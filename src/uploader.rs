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
            Ok(()) => {
                if attempt > 0 {
                    info!(
                        datastream_id = %context.datastream_id,
                        attempt = attempt + 1,
                        "upload succeeded after retry"
                    );
                }
                return Ok(());
            }
            Err(error) if error.is_conflict() => {
                // Observations already exist on the server — treat as success
                // so the cursor advances and we don't re-attempt indefinitely.
                return Ok(());
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_observation(
        job_id: &str,
        datastream_id: &str,
        row_index: u64,
    ) -> QueuedObservation {
        QueuedObservation {
            context: Arc::new(ObservationContext {
                server: Arc::new(crate::models::ServerConfig::default()),
                job_id: job_id.to_string(),
                datastream_id: datastream_id.to_string(),
                datastream_name: "Test".to_string(),
            }),
            timestamp: Utc::now(),
            row_index,
            value: json!(1.0),
        }
    }

    #[test]
    fn batch_key_groups_by_server_and_datastream() {
        let obs_a1 = test_observation("job-1", "ds-a", 1);
        let obs_a2 = test_observation("job-1", "ds-a", 2);
        let obs_b1 = test_observation("job-1", "ds-b", 1);

        let key_a1 = BatchKey::from(&obs_a1);
        let key_a2 = BatchKey::from(&obs_a2);
        let key_b1 = BatchKey::from(&obs_b1);

        assert_eq!(key_a1, key_a2, "same datastream should produce same key");
        assert_ne!(key_a1, key_b1, "different datastreams should differ");
    }

    #[test]
    fn summarize_batch_tracks_max_timestamp_and_row_index() {
        let t1 = Utc::now() - chrono::Duration::minutes(10);
        let t2 = Utc::now();

        let batch = PendingBatch {
            context: Arc::new(ObservationContext {
                server: Arc::new(crate::models::ServerConfig::default()),
                job_id: "job-1".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Test".to_string(),
            }),
            rows: vec![
                QueuedObservation {
                    context: Arc::new(ObservationContext {
                        server: Arc::new(crate::models::ServerConfig::default()),
                        job_id: "job-1".to_string(),
                        datastream_id: "ds-1".to_string(),
                        datastream_name: "Test".to_string(),
                    }),
                    timestamp: t1,
                    row_index: 5,
                    value: json!(1.0),
                },
                QueuedObservation {
                    context: Arc::new(ObservationContext {
                        server: Arc::new(crate::models::ServerConfig::default()),
                        job_id: "job-1".to_string(),
                        datastream_id: "ds-1".to_string(),
                        datastream_name: "Test".to_string(),
                    }),
                    timestamp: t2,
                    row_index: 10,
                    value: json!(2.0),
                },
            ],
        };

        let summaries = summarize_batch(&batch);
        let summary = summaries.get("job-1").expect("should have job-1");
        assert_eq!(summary.observation_count, 2);
        assert_eq!(summary.max_timestamp, t2);
        assert_eq!(summary.max_row_index, 10);
    }

    #[test]
    fn batch_size_constant_is_reasonable() {
        assert_eq!(BATCH_SIZE, 500);
        assert_eq!(FLUSH_INTERVAL, Duration::from_secs(1));
    }
}
