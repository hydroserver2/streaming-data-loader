use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
};

use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use serde_json::Value;
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::JoinHandle,
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
    event_task: StdMutex<Option<JoinHandle<()>>>,
    uploader_task: StdMutex<Option<JoinHandle<()>>>,
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

        let service = Self {
            inner: Arc::new(PipelineInner {
                config_store: config_store.clone(),
                observation_tx: Mutex::new(Some(observation_tx)),
                hydroserver: hydroserver.clone(),
                event_tx,
                watch_plan: RwLock::new(WatchPlan::default()),
                watcher: Mutex::new(None),
                row_counts: Mutex::new(HashMap::new()),
                in_flight_paths: Mutex::new(HashSet::new()),
                event_task: StdMutex::new(None),
                uploader_task: StdMutex::new(None),
            }),
        };

        service.start_background_tasks(event_rx, observation_rx);
        service
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.reload().await
    }

    pub async fn reload(&self) -> Result<(), String> {
        let snapshot = self.load_watch_plan().await?;

        {
            let mut watch_plan = self.inner.watch_plan.write().await;
            *watch_plan = snapshot.clone();
        }

        let watched_paths = snapshot.jobs_by_path.keys().cloned().collect::<Vec<_>>();
        info!(
            watched_file_count = watched_paths.len(),
            "reloading pipeline watcher"
        );
        let watcher = FilesystemWatcher::start(watched_paths.clone(), self.inner.event_tx.clone())?;
        *self.inner.watcher.lock().await = watcher;

        {
            let mut row_counts = self.inner.row_counts.lock().await;
            row_counts.retain(|path, _| snapshot.jobs_by_path.contains_key(path));
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

        if let Ok(mut slot) = self.inner.event_task.lock() {
            if let Some(task) = slot.take() {
                task.abort();
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

    pub async fn run_job_now(&self, job_id: &str) -> Result<(), String> {
        let (server, job) = self.load_manual_job(job_id).await?;
        let path = normalize_watched_path(&job.file_path);
        self.scan_job(path, server, job, ScanMode::FullResync)
            .await
            .map(|_| ())
    }

    fn start_background_tasks(
        &self,
        mut event_rx: mpsc::UnboundedReceiver<PathBuf>,
        observation_rx: ObservationReceiver,
    ) {
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

        if let Ok(mut slot) = self.inner.event_task.lock() {
            *slot = Some(event_task);
        }
        if let Ok(mut slot) = self.inner.uploader_task.lock() {
            *slot = Some(uploader_task);
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

    async fn load_manual_job(
        &self,
        job_id: &str,
    ) -> Result<(Arc<ServerConfig>, JobConfig), String> {
        let job_id = job_id.to_string();
        let config_store = self.inner.config_store.clone();
        tokio::task::spawn_blocking(move || {
            let config = config_store.load()?;
            if !config.server.is_configured() {
                return Err("HydroServer is not configured.".to_string());
            }

            let job = config
                .jobs
                .into_iter()
                .find(|job| job.id == job_id)
                .ok_or_else(|| "That job could not be found.".to_string())?;

            Ok((Arc::new(config.server.normalized()), job))
        })
        .await
        .map_err(|err| err.to_string())?
    }

    async fn scan_path(&self, path: PathBuf) -> Result<(), String> {
        let path = normalize_watched_path(path);
        if !self.begin_path_scan(&path).await {
            debug!(file = %path.display(), "skipping scan; already in flight");
            return Ok(());
        }
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
                    .scan_job(path.clone(), server.clone(), job, ScanMode::Incremental)
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
        path: PathBuf,
        server: Arc<ServerConfig>,
        job: JobConfig,
        mode: ScanMode,
    ) -> Result<usize, String> {
        let previous_row_count = {
            let row_counts = self.inner.row_counts.lock().await;
            row_counts.get(&path).copied().unwrap_or_default()
        };
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
            self.inner
                .row_counts
                .lock()
                .await
                .insert(path, result.file_row_count);
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

        self.inner
            .row_counts
            .lock()
            .await
            .insert(path, result.file_row_count);

        Ok(result.file_row_count)
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
                },
            )?;
            Ok::<(), String>(())
        })
        .await
        .map_err(|err| err.to_string())?
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

fn scan_job_file(
    job: JobConfig,
    previous_row_count: usize,
    cursor: JobCursor,
    mode: ScanMode,
) -> Result<JobScanResult, String> {
    let csv_text = fs::read_to_string(&job.file_path).map_err(|err| err.to_string())?;
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
    let start_index = match mode {
        ScanMode::Incremental if !reset_detected => data_start_index.max(previous_row_count),
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
mod tests {
    use super::{read_csv_rows, scan_job_file, ScanMode};
    use crate::models::{
        ColumnMapping, FileConfig, IdentifierType, JobConfig, JobCursor, TimestampConfig,
    };
    use chrono::Utc;

    fn sample_job(path: &str) -> JobConfig {
        JobConfig {
            id: "job-1".to_string(),
            name: "Example".to_string(),
            enabled: true,
            file_path: path.to_string(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: Some(3),
                data_start_row: 4,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Name,
                timestamp: TimestampConfig::default(),
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        }
    }

    #[test]
    fn read_csv_rows_allows_variable_width_preamble_rows() {
        let csv_text = "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
";

        let rows = read_csv_rows(csv_text, ',').expect("csv should parse");

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], vec!["Station", "Example Creek at Demo Site"]);
        assert_eq!(rows[2].len(), 3);
    }

    #[test]
    fn scan_job_file_only_returns_appended_rows() {
        let path = std::env::temp_dir().join(format!(
            "sdl-pipeline-test-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(
            &path,
            "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
        )
        .expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().expect("utf-8 path")),
            4,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("scan should succeed");

        assert_eq!(result.file_row_count, 5);
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].row_index, 5);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scan_persists_row_count_across_successive_events() {
        let path = std::env::temp_dir().join(format!(
            "sdl-pipeline-persist-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        // Initial write: 3 header/preamble rows + 2 data rows = 5 total
        std::fs::write(
            &path,
            "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
        )
        .expect("write csv");

        let job = sample_job(path.to_str().expect("utf-8 path"));

        // First scan with previous_row_count=0 sees both data rows
        let result1 = scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental)
            .expect("scan 1");
        assert_eq!(result1.file_row_count, 5);
        assert_eq!(result1.observations.len(), 2);

        // Second scan with previous_row_count=5 sees nothing new
        let result2 = scan_job_file(
            job.clone(),
            result1.file_row_count,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("scan 2");
        assert_eq!(result2.observations.len(), 0);

        // Append one row
        std::fs::write(
            &path,
            "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
",
        )
        .expect("append csv");

        // Third scan with previous_row_count=5 sees only the new row
        let result3 = scan_job_file(
            job.clone(),
            result2.file_row_count,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("scan 3");
        assert_eq!(result3.file_row_count, 6);
        assert_eq!(result3.observations.len(), 1);
        assert_eq!(result3.observations[0].row_index, 6);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scan_detects_file_truncation_and_rescans() {
        let path = std::env::temp_dir().join(format!(
            "sdl-pipeline-truncate-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        std::fs::write(
            &path,
            "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
",
        )
        .expect("write csv");

        let job = sample_job(path.to_str().expect("utf-8 path"));

        // First scan sees 3 data rows
        let result1 = scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental)
            .expect("scan 1");
        assert_eq!(result1.file_row_count, 6);
        assert_eq!(result1.observations.len(), 3);

        // Truncate and rewrite with fewer rows
        std::fs::write(
            &path,
            "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 10:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 09:00:00,2.60,8.1
",
        )
        .expect("rewrite csv");

        // Scan detects reset (4 < 6) and rescans from data_start_row
        let result2 = scan_job_file(
            job.clone(),
            result1.file_row_count,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("scan 2");
        assert!(result2.reset_detected);
        assert_eq!(result2.file_row_count, 4);
        assert_eq!(result2.observations.len(), 1);

        let _ = std::fs::remove_file(path);
    }

    // ---------------------------------------------------------------
    // Edge-case tests for real-world CSV files
    // ---------------------------------------------------------------

    /// Many environmental-monitoring loggers emit 50–200 lines of metadata
    /// (station name, serial number, units row, etc.) before the actual
    /// header + data begin.  The user sets `header_row` and `data_start_row`
    /// to skip past all of that.
    #[test]
    fn large_comment_preamble_is_skipped_correctly() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-preamble-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let mut csv = String::new();
        // 100 lines of free-form metadata
        for i in 1..=100 {
            csv.push_str(&format!("Comment line {i}: some logger metadata\n"));
        }
        // header on row 101, data starts at row 102
        csv.push_str("Timestamp,Stage_ft,WaterTemp_C\n");
        csv.push_str("2026-04-03 08:00:00,2.41,7.8\n");
        csv.push_str("2026-04-03 08:05:00,2.45,7.9\n");
        csv.push_str("2026-04-03 08:10:00,2.50,8.0\n");

        std::fs::write(&path, &csv).expect("write csv");

        let job = JobConfig {
            id: "job-preamble".to_string(),
            name: "Preamble Test".to_string(),
            enabled: true,
            file_path: path.to_str().unwrap().to_string(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: Some(101),
                data_start_row: 102,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Name,
                timestamp: TimestampConfig::default(),
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("scan with large preamble");
        assert_eq!(result.file_row_count, 104);
        assert_eq!(result.observations.len(), 3);
        assert_eq!(result.observations[0].row_index, 102);
        assert_eq!(result.observations[2].row_index, 104);

        let _ = std::fs::remove_file(path);
    }

    /// Campbell Scientific CR1000-style files have a 4-line header: station
    /// info, column names, units row, and processing description — only the
    /// second line is the "real" header.
    #[test]
    fn campbell_scientific_style_four_line_header() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-campbell-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
\"TOA5\",\"CR1000\",\"CPU:TestSite.CR1X\",\"12345\",\"CR1000.Std.32.06\",\"30490\",\"MyTable\"
\"TIMESTAMP\",\"RECORD\",\"Stage_ft\",\"WaterTemp_C\"
\"TS\",\"RN\",\"ft\",\"Deg C\"
\"\",\"\",\"Avg\",\"Avg\"
\"2026-04-03 08:00:00\",1,2.41,7.8
\"2026-04-03 08:05:00\",2,2.45,7.9
\"2026-04-03 08:10:00\",3,2.50,8.0
";

        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            id: "job-campbell".to_string(),
            name: "Campbell".to_string(),
            enabled: true,
            file_path: path.to_str().unwrap().to_string(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: Some(2),
                data_start_row: 5,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Name,
                timestamp: TimestampConfig {
                    key: "TIMESTAMP".to_string(),
                    ..TimestampConfig::default()
                },
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("scan campbell file");
        assert_eq!(result.observations.len(), 3);
        assert_eq!(result.observations[0].row_index, 5);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_csv_file_returns_zero_observations() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-empty-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        std::fs::write(&path, "").expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("empty file should not error");
        assert_eq!(result.file_row_count, 0);
        assert_eq!(result.observations.len(), 0);
        assert!(!result.reset_detected);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn header_only_file_returns_zero_observations() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-headeronly-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        std::fs::write(
            &path,
            "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
",
        )
        .expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("header-only should succeed");
        assert_eq!(result.file_row_count, 3);
        assert_eq!(result.observations.len(), 0);

        let _ = std::fs::remove_file(path);
    }

    /// Real sensor data often has gaps — e.g. a data column is blank when
    /// the sensor was offline.  Blank observation values should be skipped
    /// without breaking other columns or rows.
    #[test]
    fn sparse_rows_with_missing_values_are_handled() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-sparse-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,,7.9
2026-04-03 08:10:00,2.50,
2026-04-03 08:15:00,,
2026-04-03 08:20:00,2.55,8.1
";

        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            column_mappings: vec![
                ColumnMapping {
                    csv_column: "Stage_ft".to_string(),
                    datastream_id: "ds-stage".to_string(),
                    datastream_name: "Stage".to_string(),
                },
                ColumnMapping {
                    csv_column: "WaterTemp_C".to_string(),
                    datastream_id: "ds-temp".to_string(),
                    datastream_name: "Temp".to_string(),
                },
            ],
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("sparse scan");

        // Row 4: both present (2 obs), row 5: temp only (1), row 6: stage only (1),
        // row 7: neither (0), row 8: both (2) => 6 total
        assert_eq!(result.observations.len(), 6);

        // Verify the observations are from the right datastreams
        let stage_obs: Vec<_> = result
            .observations
            .iter()
            .filter(|o| o.datastream_id == "ds-stage")
            .collect();
        let temp_obs: Vec<_> = result
            .observations
            .iter()
            .filter(|o| o.datastream_id == "ds-temp")
            .collect();
        assert_eq!(stage_obs.len(), 3); // rows 4, 6, 8
        assert_eq!(temp_obs.len(), 3); // rows 4, 5, 8

        let _ = std::fs::remove_file(path);
    }

    /// Empty timestamp rows should be silently skipped.
    #[test]
    fn rows_with_empty_timestamps_are_skipped() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-emptyts-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
   ,2.55,8.1
2026-04-03 08:20:00,2.60,8.2
";

        std::fs::write(&path, csv).expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("empty-ts scan");

        // Only rows with valid timestamps: 4, 6, 8
        assert_eq!(result.observations.len(), 3);
        assert_eq!(result.observations[0].row_index, 4);
        assert_eq!(result.observations[1].row_index, 6);
        assert_eq!(result.observations[2].row_index, 8);

        let _ = std::fs::remove_file(path);
    }

    /// Quoted CSV fields containing the delimiter character itself.
    #[test]
    fn quoted_fields_with_embedded_commas() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-quoted-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        // The preamble has commas inside quotes; data values should still parse.
        let csv = "\
\"Station Name\",\"Example Creek, East Fork\"
\"Generated At\",\"April 3, 2026\"
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";

        std::fs::write(&path, csv).expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("quoted-fields scan");
        assert_eq!(result.observations.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// Tab-delimited files are common from certain loggers and spreadsheet
    /// exports.
    #[test]
    fn tab_delimited_file() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-tab-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "Station\tExample Creek\n\
                    Generated At\t2026-04-03\n\
                    Timestamp\tStage_ft\tWaterTemp_C\n\
                    2026-04-03 08:00:00\t2.41\t7.8\n\
                    2026-04-03 08:05:00\t2.45\t7.9\n";

        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            file_config: FileConfig {
                delimiter: "\t".to_string(),
                ..sample_job("").file_config.clone()
            },
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("tab-delimited scan");
        assert_eq!(result.observations.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// Windows tools write \r\n line endings.  The csv crate strips them,
    /// but we should verify the pipeline handles this end-to-end.
    #[test]
    fn windows_crlf_line_endings() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-crlf-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "Station,Example Creek\r\n\
                    Generated At,2026-04-03\r\n\
                    Timestamp,Stage_ft,WaterTemp_C\r\n\
                    2026-04-03 08:00:00,2.41,7.8\r\n\
                    2026-04-03 08:05:00,2.45,7.9\r\n";

        std::fs::write(&path, csv).expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("crlf scan");
        assert_eq!(result.observations.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// Some users configure jobs by column index rather than name
    /// (e.g. when there is no header row, or it's unreliable).
    #[test]
    fn index_based_column_identification() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-index-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        // No meaningful header — data starts immediately at row 1
        let csv = "\
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
";
        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            id: "job-index".to_string(),
            name: "Index Job".to_string(),
            enabled: true,
            file_path: path.to_str().unwrap().to_string(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: None,
                data_start_row: 1,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Index,
                timestamp: TimestampConfig {
                    key: "1".to_string(), // column 1 = timestamp
                    ..TimestampConfig::default()
                },
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "2".to_string(), // column 2 = Stage
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("index-based scan");
        assert_eq!(result.observations.len(), 3);

        let _ = std::fs::remove_file(path);
    }

    /// Column name lookup should be case-insensitive ("timestamp" matches
    /// "TIMESTAMP" or "Timestamp").
    #[test]
    fn case_insensitive_header_matching() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-case-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
TIMESTAMP,STAGE_FT,WATERTEMP_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";
        std::fs::write(&path, csv).expect("write csv");

        // Job config uses lowercase column names vs uppercase in file
        let job = JobConfig {
            file_config: FileConfig {
                timestamp: TimestampConfig {
                    key: "timestamp".to_string(),
                    ..TimestampConfig::default()
                },
                ..sample_job("").file_config.clone()
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("case-insensitive scan");
        assert_eq!(result.observations.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// FullResync with a cursor should skip rows that were already pushed,
    /// so a "Run Now" doesn't re-upload the full history.
    #[test]
    fn full_resync_respects_cursor() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-resync-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
2026-04-03 08:15:00,2.55,8.1
";
        std::fs::write(&path, csv).expect("write csv");

        // Cursor says we already pushed through 08:05 (row 5)
        let cursor = JobCursor {
            last_pushed_timestamp: Some(
                chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
                    .unwrap()
                    .and_hms_opt(8, 5, 0)
                    .unwrap()
                    .and_utc(),
            ),
            last_pushed_row_index: Some(5),
            last_run_at: None,
            last_error: None,
        };

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            cursor,
            ScanMode::FullResync,
        )
        .expect("full resync scan");

        // Should only return rows after the cursor: 08:10 and 08:15
        assert_eq!(result.observations.len(), 2);
        assert_eq!(result.observations[0].row_index, 6);
        assert_eq!(result.observations[1].row_index, 7);

        let _ = std::fs::remove_file(path);
    }

    /// Multiple column mappings from the same file — each data row should
    /// produce one observation per mapping (when the value is non-empty).
    #[test]
    fn multiple_column_mappings_produce_correct_observations() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-multi-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C,Conductivity
2026-04-03 08:00:00,2.41,7.8,145.2
2026-04-03 08:05:00,2.45,7.9,146.0
";
        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            column_mappings: vec![
                ColumnMapping {
                    csv_column: "Stage_ft".to_string(),
                    datastream_id: "ds-stage".to_string(),
                    datastream_name: "Stage".to_string(),
                },
                ColumnMapping {
                    csv_column: "WaterTemp_C".to_string(),
                    datastream_id: "ds-temp".to_string(),
                    datastream_name: "Temp".to_string(),
                },
                ColumnMapping {
                    csv_column: "Conductivity".to_string(),
                    datastream_id: "ds-cond".to_string(),
                    datastream_name: "Cond".to_string(),
                },
            ],
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("multi-mapping scan");

        // 2 data rows * 3 mappings = 6 observations
        assert_eq!(result.observations.len(), 6);

        let stage: Vec<_> = result.observations.iter().filter(|o| o.datastream_id == "ds-stage").collect();
        let temp: Vec<_> = result.observations.iter().filter(|o| o.datastream_id == "ds-temp").collect();
        let cond: Vec<_> = result.observations.iter().filter(|o| o.datastream_id == "ds-cond").collect();
        assert_eq!(stage.len(), 2);
        assert_eq!(temp.len(), 2);
        assert_eq!(cond.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// Values with leading/trailing whitespace should be trimmed and still
    /// parse correctly as numbers.
    #[test]
    fn whitespace_padded_values_are_trimmed() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-ws-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
  2026-04-03 08:00:00  ,  2.41  ,  7.8
  2026-04-03 08:05:00  ,  2.45  ,  7.9
";
        std::fs::write(&path, csv).expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("whitespace scan");
        assert_eq!(result.observations.len(), 2);

        // Values should parse as floats, not strings
        assert_eq!(result.observations[0].value, serde_json::json!(2.41));
        assert_eq!(result.observations[1].value, serde_json::json!(2.45));

        let _ = std::fs::remove_file(path);
    }

    /// File that doesn't exist should return a clear error, not a panic.
    #[test]
    fn missing_file_produces_clear_error() {
        let path = "/tmp/sdl-nonexistent-file-that-does-not-exist-99999.csv";
        let result = scan_job_file(
            sample_job(path),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("No such file") || msg.contains("not found") || msg.contains("cannot find"),
            "error should mention the missing file: {msg}"
        );
    }

    /// A column referenced in the mapping that doesn't exist in the header
    /// should produce a clear error pointing at the column name.
    #[test]
    fn missing_column_produces_clear_error() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-missingcol-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
";
        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            column_mappings: vec![ColumnMapping {
                csv_column: "Discharge_cfs".to_string(), // does not exist
                datastream_id: "ds-1".to_string(),
                datastream_name: "Discharge".to_string(),
            }],
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("Discharge_cfs"),
            "error should name the missing column: {msg}"
        );

        let _ = std::fs::remove_file(path);
    }

    /// Values that look like strings (e.g. "good", "suspect") should be
    /// preserved as JSON strings, while numbers become JSON numbers.
    #[test]
    fn mixed_numeric_and_string_observation_values() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-mixed-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,low,sensor_error
2026-04-03 08:10:00,-0.5,0
";
        std::fs::write(&path, csv).expect("write csv");

        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            0,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("mixed values scan");
        assert_eq!(result.observations.len(), 3);

        assert_eq!(result.observations[0].value, serde_json::json!(2.41));
        assert_eq!(
            result.observations[1].value,
            serde_json::json!("low")
        );
        assert_eq!(result.observations[2].value, serde_json::json!(-0.5));

        let _ = std::fs::remove_file(path);
    }

    /// Semicolon-delimited files (common in European locales).
    #[test]
    fn semicolon_delimited_file() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-semi-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station;Example Creek\n\
Generated At;2026-04-03\n\
Timestamp;Stage_ft;WaterTemp_C\n\
2026-04-03 08:00:00;2.41;7.8\n\
2026-04-03 08:05:00;2.45;7.9\n";

        std::fs::write(&path, csv).expect("write csv");

        let job = JobConfig {
            file_config: FileConfig {
                delimiter: ";".to_string(),
                ..sample_job("").file_config.clone()
            },
            ..sample_job(path.to_str().unwrap())
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
            .expect("semicolon scan");
        assert_eq!(result.observations.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    /// Incremental scan where previous_row_count is beyond the
    /// data_start_row but the file hasn't grown — zero observations.
    #[test]
    fn incremental_no_change_returns_zero() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-nochange-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";
        std::fs::write(&path, csv).expect("write csv");

        // Simulate having already seen all 5 rows
        let result = scan_job_file(
            sample_job(path.to_str().unwrap()),
            5,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("no-change scan");
        assert_eq!(result.observations.len(), 0);
        assert!(!result.reset_detected);

        let _ = std::fs::remove_file(path);
    }

    /// BOM-prefixed UTF-8 files (common when CSV is saved from Excel).
    /// The BOM bytes (\xEF\xBB\xBF) must not corrupt the first field.
    #[test]
    fn utf8_bom_does_not_corrupt_first_column() {
        let path = std::env::temp_dir().join(format!(
            "sdl-edge-bom-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        // Write BOM + CSV content
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"Timestamp,Stage_ft,WaterTemp_C\n");
        bytes.extend_from_slice(b"2026-04-03 08:00:00,2.41,7.8\n");
        bytes.extend_from_slice(b"2026-04-03 08:05:00,2.45,7.9\n");
        std::fs::write(&path, &bytes).expect("write bom csv");

        let job = JobConfig {
            id: "job-bom".to_string(),
            name: "BOM Test".to_string(),
            enabled: true,
            file_path: path.to_str().unwrap().to_string(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: Some(1),
                data_start_row: 2,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Name,
                timestamp: TimestampConfig::default(),
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        };

        let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental);

        // The BOM bytes become part of the first cell when read as UTF-8.
        // This is a known limitation — verify the test captures current behavior.
        // If this fails with a column-not-found error, we need a BOM-stripping fix.
        match &result {
            Ok(r) => {
                // If it works, great — both observations should be present
                assert_eq!(r.observations.len(), 2);
            }
            Err(msg) => {
                // If it fails because the BOM corrupted "Timestamp", that's a
                // real bug we need to fix.
                assert!(
                    msg.contains("Timestamp") || msg.contains("not found"),
                    "unexpected error: {msg}"
                );
                // Flag this as a known issue rather than letting it silently pass
                panic!("BOM corrupts the first header cell — needs a fix in scan_job_file: {msg}");
            }
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn large_csv_scan_produces_bounded_observations() {
        let path = std::env::temp_dir().join(format!(
            "sdl-pipeline-large-{}-{}.csv",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        // Generate a 10,000-row CSV using epoch seconds to avoid invalid dates
        let mut csv = String::from("Station,Example Creek\nGenerated At,2026-04-03\nTimestamp,Stage_ft,WaterTemp_C\n");
        let base = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        for i in 0..10_000u64 {
            let ts = base + chrono::Duration::minutes(i as i64 * 5);
            csv.push_str(&format!(
                "{},{:.2},{:.1}\n",
                ts.format("%Y-%m-%dT%H:%M:%S"),
                2.0 + (i as f64) * 0.01,
                7.0 + (i as f64) * 0.001,
            ));
        }
        std::fs::write(&path, &csv).expect("write large csv");

        let job = sample_job(path.to_str().expect("utf-8 path"));

        // Full scan from row 0 — should produce exactly 10,000 observations
        let result = scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental)
            .expect("scan large file");
        assert_eq!(result.file_row_count, 10_003); // 3 header + 10,000 data
        assert_eq!(result.observations.len(), 10_000);

        // Incremental scan with previous_row_count = full file — should produce 0
        let result2 = scan_job_file(
            job.clone(),
            result.file_row_count,
            JobCursor::default(),
            ScanMode::Incremental,
        )
        .expect("incremental scan of unchanged file");
        assert_eq!(result2.observations.len(), 0);

        let _ = std::fs::remove_file(path);
    }
}
