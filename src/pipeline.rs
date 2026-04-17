use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::Instant,
};

use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use serde_json::Value;
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tracing::{debug, error, info};

use crate::{
    config_store::ConfigStore,
    file_watcher::FilesystemWatcher,
    hydroserver::HydroServerService,
    models::{JobConfig, JobCursor, JobLogEntry, LogLevel, ServerConfig},
    observation_queue::{
        bounded, ObservationContext, ObservationReceiver, ObservationSender, QueuedObservation,
    },
    service_paths::manual_run_trigger_path,
    timestamp::parse_timestamp_to_utc,
    uploader::spawn_upload_worker,
};

const DEFAULT_QUEUE_CAPACITY: usize = 10_000;

#[derive(Clone)]
pub struct PipelineService {
    inner: Arc<PipelineInner>,
}

struct PipelineInner {
    config_store: Arc<ConfigStore>,
    observation_tx: Mutex<Option<ObservationSender>>,
    hydroserver: Arc<HydroServerService>,
    event_tx: mpsc::UnboundedSender<PathBuf>,
    watch_plan: RwLock<WatchPlan>,
    watcher: Mutex<Option<FilesystemWatcher>>,
    row_counts: Mutex<HashMap<PathBuf, usize>>,
    in_flight_paths: Mutex<HashSet<PathBuf>>,
    last_scan_times: Mutex<HashMap<PathBuf, Instant>>,
    event_task: StdMutex<Option<JoinHandle<()>>>,
    uploader_task: StdMutex<Option<JoinHandle<()>>>,
    schedule_task: StdMutex<Option<JoinHandle<()>>>,
    // Held until the first initialize() call, then consumed by start_background_tasks.
    pending_event_rx: StdMutex<Option<mpsc::UnboundedReceiver<PathBuf>>>,
    pending_observation_rx: StdMutex<Option<ObservationReceiver>>,
}

#[derive(Clone, Default)]
struct WatchPlan {
    jobs_by_path: HashMap<PathBuf, Vec<JobConfig>>,
    server: Option<Arc<ServerConfig>>,
}

#[derive(Debug)]
struct ParsedObservation {
    datastream_id: String,
    datastream_name: String,
    timestamp: DateTime<Utc>,
    row_index: u64,
    value: Value,
}

#[derive(Debug)]
struct JobScanResult {
    file_row_count: usize,
    observations: Vec<ParsedObservation>,
    reset_detected: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy)]
enum ScanMode {
    Incremental,
    FullResync,
}

impl PipelineService {
    pub fn new(config_store: Arc<ConfigStore>, hydroserver: Arc<HydroServerService>) -> Self {
        let queue_capacity = std::env::var("SDL_QUEUE_CAPACITY")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_QUEUE_CAPACITY);
        let (observation_tx, observation_rx) = bounded(queue_capacity);
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            inner: Arc::new(PipelineInner {
                config_store: config_store.clone(),
                observation_tx: Mutex::new(Some(observation_tx)),
                hydroserver: hydroserver.clone(),
                event_tx,
                watch_plan: RwLock::new(WatchPlan::default()),
                watcher: Mutex::new(None),
                row_counts: Mutex::new(HashMap::new()),
                in_flight_paths: Mutex::new(HashSet::new()),
                last_scan_times: Mutex::new(HashMap::new()),
                event_task: StdMutex::new(None),
                uploader_task: StdMutex::new(None),
                schedule_task: StdMutex::new(None),
                pending_event_rx: StdMutex::new(Some(event_rx)),
                pending_observation_rx: StdMutex::new(Some(observation_rx)),
            }),
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.start_background_tasks();
        self.reload().await
    }

    pub async fn reload(&self) -> Result<(), String> {
        let snapshot = self.load_watch_plan().await?;

        {
            let mut watch_plan = self.inner.watch_plan.write().await;
            *watch_plan = snapshot.clone();
        }

        let watched_paths = snapshot.jobs_by_path.keys().cloned().collect::<Vec<_>>();
        let manual_trigger_targets = snapshot
            .jobs_by_path
            .values()
            .flat_map(|jobs| {
                jobs.iter().filter_map(|job| {
                    manual_run_trigger_path(&job.id, &job.file_path)
                        .ok()
                        .map(|trigger_path| (trigger_path, normalize_watched_path(&job.file_path)))
                })
            })
            .collect::<Vec<_>>();
        info!(
            watched_file_count = watched_paths.len(),
            "reloading pipeline watcher"
        );
        let watcher = FilesystemWatcher::start(
            watched_paths.clone(),
            manual_trigger_targets,
            self.inner.event_tx.clone(),
        )?;
        *self.inner.watcher.lock().await = watcher;

        // Seed row_counts from persisted cursors for paths not yet tracked in memory.
        // This prevents re-uploading the entire file history after a process restart.
        let seeds = self.load_cursor_row_seeds(&snapshot).await;
        {
            let mut row_counts = self.inner.row_counts.lock().await;
            row_counts.retain(|path, _| snapshot.jobs_by_path.contains_key(path));
            for (path, seed) in seeds {
                row_counts.entry(path).or_insert(seed);
            }
        }

        for path in &watched_paths {
            debug!(file = %path.display(), "queuing initial scan");
        }
        for path in watched_paths {
            let _ = self.inner.event_tx.send(path);
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        info!("pipeline shutting down; draining pending uploads");
        *self.inner.watcher.lock().await = None;

        for slot in [&self.inner.event_task, &self.inner.schedule_task] {
            if let Ok(mut guard) = slot.lock() {
                if let Some(task) = guard.take() {
                    task.abort();
                }
            }
        }

        // Drop the observation sender so the uploader sees channel-closed and
        // drains its remaining batches instead of waiting forever.
        self.inner.observation_tx.lock().await.take();

        let uploader_task = self
            .inner
            .uploader_task
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());

        if let Some(task) = uploader_task {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(30), task).await;
        }
    }

    fn start_background_tasks(&self) {
        let (mut event_rx, observation_rx) = match (
            self.inner
                .pending_event_rx
                .lock()
                .ok()
                .and_then(|mut g| g.take()),
            self.inner
                .pending_observation_rx
                .lock()
                .ok()
                .and_then(|mut g| g.take()),
        ) {
            (Some(e), Some(o)) => (e, o),
            _ => return, // already started
        };

        let service = self.clone();
        let event_task = tokio::spawn(async move {
            while let Some(path) = event_rx.recv().await {
                if let Err(error) = service.scan_path(path).await {
                    error!(error = %error, "filesystem-triggered scan failed");
                }
            }
        });

        let uploader_task = spawn_upload_worker(
            observation_rx,
            self.inner.hydroserver.clone(),
            self.inner.config_store.clone(),
        );

        // Scheduler: every 60 s, re-queue any path whose jobs are overdue per
        // their schedule_minutes setting.  This catches files on network drives
        // or other filesystems that don't reliably fire OS change events.
        let service = self.clone();
        let schedule_task = tokio::spawn(async move {
            const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
            let mut ticker = interval(POLL_INTERVAL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;

                let overdue_paths = {
                    let watch_plan = service.inner.watch_plan.read().await;
                    let last_scan_times = service.inner.last_scan_times.lock().await;
                    overdue_paths(Instant::now(), &watch_plan, &last_scan_times)
                };

                for path in overdue_paths {
                    debug!(file = %path.display(), "scheduled poll triggered scan");
                    let _ = service.inner.event_tx.send(path);
                }
            }
        });

        if let Ok(mut slot) = self.inner.event_task.lock() {
            *slot = Some(event_task);
        }
        if let Ok(mut slot) = self.inner.uploader_task.lock() {
            *slot = Some(uploader_task);
        }
        if let Ok(mut slot) = self.inner.schedule_task.lock() {
            *slot = Some(schedule_task);
        }
    }

    async fn load_watch_plan(&self) -> Result<WatchPlan, String> {
        let config_store = self.inner.config_store.clone();
        tokio::task::spawn_blocking(move || {
            let config = config_store.load()?;
            if !config.server.is_configured() {
                return Ok(WatchPlan::default());
            }

            let server = Arc::new(config.server.clone().normalized());
            let jobs_by_path = config.jobs.into_iter().filter(|job| job.enabled).fold(
                HashMap::new(),
                |mut acc, job| {
                    acc.entry(normalize_watched_path(&job.file_path))
                        .or_insert_with(Vec::new)
                        .push(job);
                    acc
                },
            );

            Ok(WatchPlan {
                jobs_by_path,
                server: Some(server),
            })
        })
        .await
        .map_err(|err| err.to_string())?
    }

    /// Returns the minimum confirmed `last_pushed_row_index` across all jobs for
    /// each watched path.  Used to seed `row_counts` on startup so incremental
    /// scans resume from where they left off instead of re-uploading everything.
    async fn load_cursor_row_seeds(&self, snapshot: &WatchPlan) -> HashMap<PathBuf, usize> {
        let config_store = self.inner.config_store.clone();
        let pairs: Vec<(PathBuf, Vec<String>)> = snapshot
            .jobs_by_path
            .iter()
            .map(|(path, jobs)| (path.clone(), jobs.iter().map(|j| j.id.clone()).collect()))
            .collect();

        tokio::task::spawn_blocking(move || {
            let mut seeds: HashMap<PathBuf, usize> = HashMap::new();
            for (path, job_ids) in pairs {
                for job_id in &job_ids {
                    if let Ok(cursor) = config_store.cursor_for(job_id) {
                        if let Some(row_index) = cursor.last_pushed_row_index {
                            let entry = seeds.entry(path.clone()).or_insert(row_index as usize);
                            // Use the minimum across all jobs sharing this path so the
                            // slowest job drives where the scan resumes.
                            *entry = (*entry).min(row_index as usize);
                        }
                    }
                }
            }
            seeds
        })
        .await
        .unwrap_or_default()
    }

    async fn scan_path(&self, path: PathBuf) -> Result<(), String> {
        let path = normalize_watched_path(path);
        if !self.begin_path_scan(&path).await {
            debug!(file = %path.display(), "skipping scan; already in flight");
            return Ok(());
        }
        // Record scan start time so the scheduler can determine when to retry.
        self.inner
            .last_scan_times
            .lock()
            .await
            .insert(path.clone(), Instant::now());
        debug!(file = %path.display(), "scanning watched file for new rows");

        let outcome = async {
            let snapshot = {
                let watch_plan = self.inner.watch_plan.read().await;
                let jobs = watch_plan
                    .jobs_by_path
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();
                let server = watch_plan.server.clone();
                (server, jobs)
            };

            let (Some(server), jobs) = snapshot else {
                return Ok(());
            };

            if jobs.is_empty() {
                return Ok(());
            }

            let previous_row_count = {
                let row_counts = self.inner.row_counts.lock().await;
                row_counts.get(&path).copied().unwrap_or_default()
            };

            let mut latest_row_count = previous_row_count;
            for job in jobs {
                match self
                    .scan_job(
                        path.clone(),
                        server.clone(),
                        job,
                        previous_row_count,
                        ScanMode::Incremental,
                    )
                    .await
                {
                    Ok(row_count) => {
                        latest_row_count = latest_row_count.max(row_count);
                    }
                    Err(error) => {
                        latest_row_count = latest_row_count.max(previous_row_count);
                        error!(file = %path.display(), error = %error, "job scan failed");
                    }
                }
            }

            self.inner
                .row_counts
                .lock()
                .await
                .insert(path.clone(), latest_row_count);

            Ok(())
        }
        .await;

        self.end_path_scan(&path).await;
        outcome
    }

    async fn scan_job(
        &self,
        _path: PathBuf,
        server: Arc<ServerConfig>,
        job: JobConfig,
        previous_row_count: usize,
        mode: ScanMode,
    ) -> Result<usize, String> {
        self.set_job_running(&job.id, true).await?;

        let outcome = async {
            let cursor = self.load_cursor(&job.id).await?;
            let job_for_scan = job.clone();

            let result = tokio::task::spawn_blocking(move || {
                scan_job_file(job_for_scan, previous_row_count, cursor, mode)
            })
            .await
            .map_err(|err| err.to_string())??;

            if result.reset_detected {
                self.append_log(
                    &job.id,
                    "Detected that the watched CSV file was replaced or truncated; rescanning from the configured data start row.",
                    LogLevel::Warning,
                )
                .await?;
            }

            if result.observations.is_empty() {
                self.clear_last_error(&job.id).await?;
                if matches!(mode, ScanMode::FullResync) {
                    self.append_log(
                        &job.id,
                        "No new observations were available to queue.",
                        LogLevel::Info,
                    )
                    .await?;
                }
                return Ok(result.file_row_count);
            }

            let mut queued = 0usize;
            for observation in result.observations {
                let context = Arc::new(ObservationContext {
                    server: server.clone(),
                    job_id: job.id.clone(),
                    datastream_id: observation.datastream_id,
                    datastream_name: observation.datastream_name,
                });
                let tx = self.inner.observation_tx.lock().await;
                let Some(tx) = tx.as_ref() else {
                    return Err("Pipeline is shutting down.".to_string());
                };
                tx.send(QueuedObservation {
                    context,
                    timestamp: observation.timestamp,
                    row_index: observation.row_index,
                    value: observation.value,
                })
                .await?;
                queued += 1;
            }

            self.clear_last_error(&job.id).await?;

            info!(
                job_id = %job.id,
                queued_count = queued,
                file = %job.file_path,
                "queued observations from watched CSV file"
            );

            Ok(result.file_row_count)
        }
        .await;

        let running_clear = self.set_job_running(&job.id, false).await;
        if let Err(error) = running_clear {
            return Err(error);
        }

        outcome
    }

    async fn begin_path_scan(&self, path: &Path) -> bool {
        let mut in_flight = self.inner.in_flight_paths.lock().await;
        in_flight.insert(path.to_path_buf())
    }

    async fn end_path_scan(&self, path: &Path) {
        self.inner.in_flight_paths.lock().await.remove(path);
    }

    async fn load_cursor(&self, job_id: &str) -> Result<JobCursor, String> {
        let config_store = self.inner.config_store.clone();
        let job_id = job_id.to_string();
        tokio::task::spawn_blocking(move || config_store.cursor_for(&job_id))
            .await
            .map_err(|err| err.to_string())?
    }

    async fn clear_last_error(&self, job_id: &str) -> Result<(), String> {
        let config_store = self.inner.config_store.clone();
        let job_id = job_id.to_string();
        tokio::task::spawn_blocking(move || {
            let existing = config_store.cursor_for(&job_id)?;
            config_store.update_cursor(
                &job_id,
                JobCursor {
                    last_run_at: Some(Utc::now()),
                    last_pushed_timestamp: existing.last_pushed_timestamp,
                    last_pushed_row_index: existing.last_pushed_row_index,
                    last_error: None,
                    is_running: existing.is_running,
                },
            )?;
            Ok::<(), String>(())
        })
        .await
        .map_err(|err| err.to_string())?
    }

    async fn set_job_running(&self, job_id: &str, is_running: bool) -> Result<(), String> {
        let config_store = self.inner.config_store.clone();
        let job_id = job_id.to_string();
        tokio::task::spawn_blocking(move || config_store.set_job_running(&job_id, is_running))
            .await
            .map_err(|err| err.to_string())?
            .map(|_| ())
    }

    async fn append_log(&self, job_id: &str, message: &str, level: LogLevel) -> Result<(), String> {
        let config_store = self.inner.config_store.clone();
        let job_id = job_id.to_string();
        let message = message.to_string();
        tokio::task::spawn_blocking(move || {
            config_store.append_log(
                &job_id,
                JobLogEntry {
                    timestamp: Utc::now(),
                    level,
                    message,
                },
            )?;
            Ok::<(), String>(())
        })
        .await
        .map_err(|err| err.to_string())?
    }
}

fn overdue_paths(
    now: Instant,
    watch_plan: &WatchPlan,
    last_scan_times: &HashMap<PathBuf, Instant>,
) -> Vec<PathBuf> {
    watch_plan
        .jobs_by_path
        .iter()
        .filter_map(|(path, jobs)| {
            let min_interval_secs = jobs
                .iter()
                .map(|job| job.schedule_minutes as u64 * 60)
                .min()
                .unwrap_or(u64::MAX);

            let Some(last_scan) = last_scan_times.get(path) else {
                // Initial scan (queued by reload) handles the first run.
                return None;
            };

            (now.duration_since(*last_scan).as_secs() >= min_interval_secs).then(|| path.clone())
        })
        .collect()
}

fn scan_job_file(
    job: JobConfig,
    previous_row_count: usize,
    cursor: JobCursor,
    mode: ScanMode,
) -> Result<JobScanResult, String> {
    let bytes = fs::read(&job.file_path).map_err(|err| err.to_string())?;
    let (csv_text, _encoding) = crate::csv_preview::decode_text(&bytes)?;
    let delimiter = job
        .file_config
        .delimiter
        .chars()
        .next()
        .ok_or_else(|| "Delimiter is required.".to_string())?;
    let rows = read_csv_rows(&csv_text, delimiter)?;
    let file_row_count = rows.len();

    if rows.is_empty() {
        return Ok(JobScanResult {
            file_row_count,
            observations: Vec::new(),
            reset_detected: false,
        });
    }

    let data_start_index = job.file_config.data_start_row.saturating_sub(1) as usize;
    let timestamp_index = resolve_column_index(&rows, &job, &job.file_config.timestamp.key)?;
    let mapping_indexes = job
        .column_mappings
        .iter()
        .map(|mapping| {
            resolve_column_index(&rows, &job, &mapping.csv_column).map(|index| {
                (
                    mapping.datastream_id.clone(),
                    mapping.datastream_name.clone(),
                    index,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let reset_detected =
        matches!(mode, ScanMode::Incremental) && file_row_count < previous_row_count;
    // When the previous upload failed, backtrack to the last confirmed push so
    // those rows are retried.  Otherwise use the in-memory row count as usual.
    let incremental_start = if cursor.last_error.is_some() {
        cursor
            .last_pushed_row_index
            .map(|i| i as usize)
            .unwrap_or(0)
    } else {
        previous_row_count
    };
    let start_index = match mode {
        ScanMode::Incremental if !reset_detected => data_start_index.max(incremental_start),
        ScanMode::Incremental | ScanMode::FullResync => data_start_index,
    };

    let mut observations = Vec::new();
    for (row_number, row) in rows.iter().enumerate().skip(start_index) {
        let csv_row_number = row_number as u64 + 1;
        let timestamp_value = row
            .get(timestamp_index)
            .map(String::as_str)
            .unwrap_or_default()
            .trim();
        if timestamp_value.is_empty() {
            continue;
        }

        let timestamp = parse_timestamp_to_utc(timestamp_value, &job.file_config.timestamp)
            .map_err(|error| format!("Row {csv_row_number}: {error}"))?;

        if matches!(mode, ScanMode::FullResync)
            && !is_newer_than_cursor(timestamp, csv_row_number, &cursor)
        {
            continue;
        }

        for (datastream_id, datastream_name, column_index) in &mapping_indexes {
            let value = row
                .get(*column_index)
                .map(String::as_str)
                .unwrap_or_default()
                .trim();
            if value.is_empty() {
                continue;
            }

            observations.push(ParsedObservation {
                datastream_id: datastream_id.clone(),
                datastream_name: datastream_name.clone(),
                timestamp,
                row_index: csv_row_number,
                value: parse_observation_value(value),
            });
        }
    }

    Ok(JobScanResult {
        file_row_count,
        observations,
        reset_detected,
    })
}

fn resolve_column_index(rows: &[Vec<String>], job: &JobConfig, key: &str) -> Result<usize, String> {
    if job.file_config.identifier_type == crate::models::IdentifierType::Index {
        let index = key
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Column index '{key}' is invalid."))?;
        if index == 0 {
            return Err(format!("Column index '{key}' is invalid."));
        }
        return Ok(index - 1);
    }

    let header_row = job.file_config.header_row.ok_or_else(|| {
        "headerRow is required when using name-based column identifiers.".to_string()
    })?;
    let header_index = header_row.saturating_sub(1) as usize;
    let header = rows
        .get(header_index)
        .ok_or_else(|| "The configured header row does not exist in the file.".to_string())?;

    let target = key.trim();
    header
        .iter()
        .position(|value| value.trim() == target)
        .or_else(|| {
            header
                .iter()
                .position(|value| value.trim().eq_ignore_ascii_case(target))
        })
        .ok_or_else(|| {
            format!(
                "Column '{key}' was not found in the configured header row. Confirm the delimiter and headerRow settings match the source file."
            )
        })
}

fn read_csv_rows(csv_text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let delimiter = delimiter as u8;
    ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(csv_text.as_bytes())
        .records()
        .map(|record| {
            record
                .map(|record| record.iter().map(|value| value.to_string()).collect())
                .map_err(|err| err.to_string())
        })
        .collect()
}

fn parse_observation_value(value: &str) -> Value {
    value
        .parse::<f64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::String(value.to_string()))
}

fn normalize_watched_path(path: impl AsRef<Path>) -> PathBuf {
    let path = PathBuf::from(path.as_ref());
    path.canonicalize().unwrap_or(path)
}

fn is_newer_than_cursor(timestamp: DateTime<Utc>, row_index: u64, cursor: &JobCursor) -> bool {
    match cursor.last_pushed_timestamp {
        Some(last_timestamp) if timestamp < last_timestamp => false,
        Some(last_timestamp) if timestamp == last_timestamp => cursor
            .last_pushed_row_index
            .map(|last_row_index| row_index > last_row_index)
            .unwrap_or(false),
        _ => true,
    }
}

#[cfg(test)]
#[path = "tests/pipeline.rs"]
mod tests;
