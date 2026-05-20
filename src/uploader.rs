use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use governor::{clock::DefaultClock, state::InMemoryState, Quota, RateLimiter};
use tokio::{
    task::JoinHandle,
    time::{interval, sleep, MissedTickBehavior},
};
use tracing::{error, info, warn};

use crate::{
    config_store::ConfigStore,
    hydroserver::{HydroServerService, ObservationPayloadRow},
    models::LogLevel,
    observation_queue::{ObservationContext, ObservationReceiver, QueuedObservation},
};

const BATCH_SIZE: usize = 500;
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_REQUESTS_PER_SECOND: u32 = 10;
const MAX_RETRIES: usize = 3;

type DirectRateLimiter =
    RateLimiter<governor::state::direct::NotKeyed, InMemoryState, DefaultClock>;

pub fn spawn_upload_worker(
    mut receiver: ObservationReceiver,
    hydroserver: Arc<HydroServerService>,
    config_store: Arc<ConfigStore>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let rps = std::env::var("SDL_REQUESTS_PER_SECOND")
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_REQUESTS_PER_SECOND);
        let rate_limiter = RateLimiter::direct(Quota::per_second(
            NonZeroU32::new(rps).expect("non-zero rate"),
        ));
        let mut batches: HashMap<BatchKey, PendingBatch> = HashMap::new();
        let mut flush_timer = interval(FLUSH_INTERVAL);
        flush_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_item = receiver.recv() => {
                    match maybe_item {
                        Some(item) => {
                            let key = BatchKey::from(&item);
                            let batch = batches.entry(key.clone()).or_insert_with(|| PendingBatch {
                                context: item.context.clone(),
                                rows: Vec::new(),
                            });
                            batch.rows.push(item);

                            if batch.rows.len() >= BATCH_SIZE {
                                if let Some(batch) = batches.remove(&key) {
                                    flush_batch(batch, &hydroserver, &config_store, &rate_limiter).await;
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = flush_timer.tick() => {
                    if batches.is_empty() {
                        continue;
                    }

                    let pending = batches.drain().map(|(_, batch)| batch).collect::<Vec<_>>();
                    for batch in pending {
                        flush_batch(batch, &hydroserver, &config_store, &rate_limiter).await;
                    }
                }
            }
        }

        let pending = batches.drain().map(|(_, batch)| batch).collect::<Vec<_>>();
        for batch in pending {
            flush_batch(batch, &hydroserver, &config_store, &rate_limiter).await;
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BatchKey {
    server_signature: String,
    datastream_id: String,
}

impl BatchKey {
    fn from(item: &QueuedObservation) -> Self {
        let server = item.context.server.as_ref();
        let credential = match server.auth_type {
            crate::models::AuthType::Apikey => server.api_key.trim(),
            crate::models::AuthType::Userpass => server.username.trim(),
        };

        Self {
            server_signature: format!(
                "{:?}|{}|{}|{}",
                server.auth_type, server.url, server.workspace_id, credential
            ),
            datastream_id: item.context.datastream_id.clone(),
        }
    }
}

struct PendingBatch {
    context: Arc<ObservationContext>,
    rows: Vec<QueuedObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadOutcome {
    Accepted,
    Conflict,
}

async fn flush_batch(
    batch: PendingBatch,
    hydroserver: &Arc<HydroServerService>,
    config_store: &Arc<ConfigStore>,
    rate_limiter: &DirectRateLimiter,
) {
    if batch.rows.is_empty() {
        return;
    }

    let rows: Vec<&QueuedObservation> = batch.rows.iter().collect();
    match upload_rows(hydroserver, &batch.context, &rows, rate_limiter).await {
        Ok(UploadOutcome::Accepted) => {
            info!(
                job_id = %batch.context.job_id,
                datastream_id = %batch.context.datastream_id,
                observation_count = rows.len(),
                "uploaded observation batch"
            );
            persist_rows_success(config_store.clone(), &batch.context, &rows).await;
        }
        Ok(UploadOutcome::Conflict) => {
            reconcile_conflict(
                &batch.context,
                &rows,
                hydroserver,
                config_store,
                rate_limiter,
            )
            .await;
        }
        Err(message) => {
            error!(
                job_id = %batch.context.job_id,
                datastream_id = %batch.context.datastream_id,
                observation_count = rows.len(),
                error = %message,
                "failed to upload observation batch"
            );
            persist_rows_failure(config_store.clone(), &batch.context, &rows, &message).await;
        }
    }
}

/// Reconcile a bulk-insert conflict against the server's authoritative state.
///
/// HydroServer's `bulk-create` is all-or-nothing: if any row's `phenomenonTime`
/// already exists on the datastream, the entire batch is rejected with 409 and
/// nothing is inserted. Treating that as success (the old behavior) would
/// advance the cursor past rows the server never stored, silently losing them.
/// Instead we ask the server for the datastream's `phenomenonEndTime` (its
/// latest stored observation): rows at or before it are confirmed durable and
/// the cursor may advance over them; rows after it were not stored and are
/// re-sent now — being strictly newer than the server's latest, they cannot
/// conflict.
async fn reconcile_conflict(
    context: &ObservationContext,
    rows: &[&QueuedObservation],
    hydroserver: &Arc<HydroServerService>,
    config_store: &Arc<ConfigStore>,
    rate_limiter: &DirectRateLimiter,
) {
    rate_limiter.until_ready().await;
    let watermark = match hydroserver
        .fetch_phenomenon_end_time(context.server.as_ref(), &context.datastream_id)
        .await
    {
        Ok(watermark) => watermark,
        Err(error) => {
            // We can't confirm what the server already has, so we must not
            // advance the cursor. Record a retryable failure and try again later.
            let message = format!(
                "Upload conflicted and the server's stored range could not be confirmed: {error}"
            );
            warn!(
                job_id = %context.job_id,
                datastream_id = %context.datastream_id,
                error = %error,
                "conflict reconciliation could not fetch the server watermark; will retry"
            );
            persist_rows_failure(config_store.clone(), context, rows, &message).await;
            return;
        }
    };

    let Some(watermark) = watermark else {
        // The server reports no observations yet a conflict occurred. Intra-batch
        // duplicate timestamps are collapsed before sending, so this is not
        // expected; record a retryable failure rather than risk advancing the
        // cursor over unstored rows.
        let message =
            "Upload conflicted but the server reports no stored observations to reconcile against."
                .to_string();
        warn!(
            job_id = %context.job_id,
            datastream_id = %context.datastream_id,
            "conflict reconciliation found no server watermark; will retry"
        );
        persist_rows_failure(config_store.clone(), context, rows, &message).await;
        return;
    };

    let (confirmed, pending): (Vec<&QueuedObservation>, Vec<&QueuedObservation>) =
        rows.iter().partition(|row| row.timestamp <= watermark);

    if !confirmed.is_empty() {
        info!(
            job_id = %context.job_id,
            datastream_id = %context.datastream_id,
            confirmed_count = confirmed.len(),
            watermark = %watermark,
            "reconciled conflict: observations already present on the server"
        );
        persist_rows_success(config_store.clone(), context, &confirmed).await;
    }

    if pending.is_empty() {
        return;
    }

    // Rows after the server's watermark were not stored. Re-send just those.
    match upload_rows(hydroserver, context, &pending, rate_limiter).await {
        Ok(UploadOutcome::Accepted) => {
            info!(
                job_id = %context.job_id,
                datastream_id = %context.datastream_id,
                observation_count = pending.len(),
                "uploaded observations that were missing after conflict reconciliation"
            );
            persist_rows_success(config_store.clone(), context, &pending).await;
        }
        Ok(UploadOutcome::Conflict) => {
            let message =
                "Observations still conflicted after reconciling against the server.".to_string();
            persist_rows_failure(config_store.clone(), context, &pending, &message).await;
        }
        Err(message) => {
            persist_rows_failure(config_store.clone(), context, &pending, &message).await;
        }
    }
}

async fn upload_rows(
    hydroserver: &Arc<HydroServerService>,
    context: &ObservationContext,
    rows: &[&QueuedObservation],
    rate_limiter: &DirectRateLimiter,
) -> Result<UploadOutcome, String> {
    if rows.is_empty() {
        return Ok(UploadOutcome::Accepted);
    }
    let payload = build_payload(rows);
    upload_with_retry(hydroserver, context, &payload, rate_limiter).await
}

/// Build the upload payload, collapsing duplicate `phenomenonTime` values within
/// the batch (last value wins) so a repeated timestamp in the source file can't
/// trigger a self-inflicted bulk-insert conflict. The result is ordered by
/// timestamp.
fn build_payload(rows: &[&QueuedObservation]) -> Vec<ObservationPayloadRow> {
    let mut by_time: BTreeMap<DateTime<Utc>, serde_json::Value> = BTreeMap::new();
    for row in rows {
        by_time.insert(row.timestamp, row.value.clone());
    }
    by_time
        .into_iter()
        .map(|(phenomenon_time, result)| ObservationPayloadRow {
            phenomenon_time,
            result,
        })
        .collect()
}

async fn upload_with_retry(
    hydroserver: &Arc<HydroServerService>,
    context: &ObservationContext,
    payload: &[ObservationPayloadRow],
    rate_limiter: &DirectRateLimiter,
) -> Result<UploadOutcome, String> {
    let mut backoff = Duration::from_millis(500);

    for attempt in 0..=MAX_RETRIES {
        rate_limiter.until_ready().await;
        match hydroserver
            .post_observations_batch(context.server.as_ref(), &context.datastream_id, payload)
            .await
        {
            Ok(()) => {
                if attempt > 0 {
                    info!(
                        datastream_id = %context.datastream_id,
                        attempt = attempt + 1,
                        "upload succeeded after retry"
                    );
                }
                return Ok(UploadOutcome::Accepted);
            }
            // A conflict is not success: the server stored none of these rows.
            // Hand off to reconciliation, which advances the cursor only over
            // rows the server confirms it already has.
            Err(error) if error.is_conflict() => return Ok(UploadOutcome::Conflict),
            Err(error) if error.is_retryable() && attempt < MAX_RETRIES => {
                let jitter = jitter_duration(backoff);
                let delay = backoff + jitter;
                warn!(
                    datastream_id = %context.datastream_id,
                    attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    error = %error,
                    "upload attempt failed with a retryable error"
                );
                sleep(delay).await;
                backoff *= 2;
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    Err("Observation upload failed after retries.".to_string())
}

/// Returns a jitter of 0..25% of the base duration, derived from system time nanos.
fn jitter_duration(base: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let jitter_fraction = (nanos % 250) as f64 / 1000.0; // 0.0 to 0.249
    Duration::from_secs_f64(base.as_secs_f64() * jitter_fraction)
}

async fn persist_rows_success(
    config_store: Arc<ConfigStore>,
    context: &ObservationContext,
    rows: &[&QueuedObservation],
) {
    let datastream_id = context.datastream_id.clone();
    let datastream_name = context.datastream_name.clone();

    for update in summarize_rows(rows).into_values() {
        let config_store = config_store.clone();
        let datastream_id = datastream_id.clone();
        let datastream_name = datastream_name.clone();
        let observation_count = update.observation_count;
        let _ = tokio::task::spawn_blocking(move || {
            config_store.record_datastream_success(
                &update.job_id,
                &datastream_id,
                update.max_row_index,
                update.max_timestamp,
                Utc::now(),
            )?;
            config_store.append_log(
                &update.job_id,
                crate::models::JobLogEntry {
                    timestamp: Utc::now(),
                    level: LogLevel::Info,
                    message: format!(
                        "Loaded {observation_count} observation(s) to datastream {datastream_name}."
                    ),
                },
            )?;
            Ok::<(), String>(())
        })
        .await;
    }
}

async fn persist_rows_failure(
    config_store: Arc<ConfigStore>,
    context: &ObservationContext,
    rows: &[&QueuedObservation],
    message: &str,
) {
    let datastream_id = context.datastream_id.clone();
    let message = message.to_string();

    for update in summarize_rows(rows).into_values() {
        let config_store = config_store.clone();
        let datastream_id = datastream_id.clone();
        let message = message.clone();
        let _ = tokio::task::spawn_blocking(move || {
            config_store.record_datastream_failure(
                &update.job_id,
                &datastream_id,
                &message,
                Utc::now(),
            )?;
            config_store.append_log(
                &update.job_id,
                crate::models::JobLogEntry {
                    timestamp: Utc::now(),
                    level: LogLevel::Error,
                    message: message.clone(),
                },
            )?;
            Ok::<(), String>(())
        })
        .await;
    }
}

fn summarize_rows(rows: &[&QueuedObservation]) -> HashMap<String, JobUploadSummary> {
    let mut updates = HashMap::new();
    for row in rows {
        let entry = updates
            .entry(row.context.job_id.clone())
            .or_insert_with(|| JobUploadSummary {
                job_id: row.context.job_id.clone(),
                max_timestamp: row.timestamp,
                max_row_index: row.row_index,
                observation_count: 0,
            });

        if row.timestamp > entry.max_timestamp {
            entry.max_timestamp = row.timestamp;
        }
        if row.row_index > entry.max_row_index {
            entry.max_row_index = row.row_index;
        }
        entry.observation_count += 1;
    }
    updates
}

struct JobUploadSummary {
    job_id: String,
    max_timestamp: DateTime<Utc>,
    max_row_index: u64,
    observation_count: usize,
}

#[cfg(test)]
#[path = "tests/uploader.rs"]
mod tests;
