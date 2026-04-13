use std::{collections::HashMap, num::NonZeroU32, sync::Arc, time::Duration};

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
    models::{JobCursor, LogLevel},
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
        let rate_limiter = RateLimiter::direct(Quota::per_second(
            NonZeroU32::new(DEFAULT_REQUESTS_PER_SECOND).expect("non-zero rate"),
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

async fn flush_batch(
    batch: PendingBatch,
    hydroserver: &Arc<HydroServerService>,
    config_store: &Arc<ConfigStore>,
    rate_limiter: &DirectRateLimiter,
) {
    if batch.rows.is_empty() {
        return;
    }

    rate_limiter.until_ready().await;

    let payload = batch
        .rows
        .iter()
        .map(|row| ObservationPayloadRow {
            phenomenon_time: row.timestamp,
            result: row.value.clone(),
        })
        .collect::<Vec<_>>();

    let result = upload_with_retry(hydroserver, &batch.context, &payload).await;
    match result {
        Ok(()) => {
            info!(
                job_id = %batch.context.job_id,
                datastream_id = %batch.context.datastream_id,
                observation_count = batch.rows.len(),
                "uploaded observation batch"
            );
            persist_success(config_store.clone(), &batch).await;
        }
        Err(message) => {
            error!(
                job_id = %batch.context.job_id,
                datastream_id = %batch.context.datastream_id,
                observation_count = batch.rows.len(),
                error = %message,
                "failed to upload observation batch"
            );
            persist_failure(config_store.clone(), &batch, &message).await;
        }
    }
}

async fn upload_with_retry(
    hydroserver: &Arc<HydroServerService>,
    context: &ObservationContext,
    payload: &[ObservationPayloadRow],
) -> Result<(), String> {
    let mut backoff = Duration::from_millis(500);

    for attempt in 0..=MAX_RETRIES {
        match hydroserver
            .post_observations_batch(context.server.as_ref(), &context.datastream_id, payload)
            .await
        {
            Ok(()) => return Ok(()),
            Err(error) if error.is_retryable() && attempt < MAX_RETRIES => {
                warn!(
                    datastream_id = %context.datastream_id,
                    attempt = attempt + 1,
                    delay_ms = backoff.as_millis(),
                    error = %error,
                    "upload attempt failed with a retryable error"
                );
                sleep(backoff).await;
                backoff *= 2;
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    Err("Observation upload failed after retries.".to_string())
}

async fn persist_success(config_store: Arc<ConfigStore>, batch: &PendingBatch) {
    let updates = summarize_batch(batch);

    for update in updates.into_values() {
        let config_store = config_store.clone();
        let datastream_name = batch.context.datastream_name.clone();
        let observation_count = update.observation_count;
        let _ = tokio::task::spawn_blocking(move || {
            let existing = config_store.cursor_for(&update.job_id)?;
            config_store.update_cursor(
                &update.job_id,
                JobCursor {
                    last_run_at: Some(Utc::now()),
                    last_pushed_timestamp: Some(
                        max_timestamp(existing.last_pushed_timestamp, Some(update.max_timestamp))
                            .expect("timestamp should exist"),
                    ),
                    last_pushed_row_index: Some(
                        existing
                            .last_pushed_row_index
                            .map(|current| current.max(update.max_row_index))
                            .unwrap_or(update.max_row_index),
                    ),
                    last_error: None,
                },
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

async fn persist_failure(config_store: Arc<ConfigStore>, batch: &PendingBatch, message: &str) {
    let message = message.to_string();
    let updates = summarize_batch(batch);

    for update in updates.into_values() {
        let config_store = config_store.clone();
        let message = message.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let existing = config_store.cursor_for(&update.job_id)?;
            config_store.update_cursor(
                &update.job_id,
                JobCursor {
                    last_run_at: Some(Utc::now()),
                    last_pushed_timestamp: existing.last_pushed_timestamp,
                    last_pushed_row_index: existing.last_pushed_row_index,
                    last_error: Some(message.clone()),
                },
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

fn summarize_batch(batch: &PendingBatch) -> HashMap<String, JobUploadSummary> {
    let mut updates = HashMap::new();
    for row in &batch.rows {
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

fn max_timestamp(
    left: Option<DateTime<Utc>>,
    right: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

struct JobUploadSummary {
    job_id: String,
    max_timestamp: DateTime<Utc>,
    max_row_index: u64,
    observation_count: usize,
}
