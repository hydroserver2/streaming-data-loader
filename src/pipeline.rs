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
use tracing::{error, info};

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
    observation_tx: ObservationSender,
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

struct ParsedObservation {
    datastream_id: String,
    datastream_name: String,
    timestamp: DateTime<Utc>,
    row_index: u64,
    value: Value,
}

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
                observation_tx,
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
        let watcher = FilesystemWatcher::start(watched_paths.clone(), self.inner.event_tx.clone())?;
        *self.inner.watcher.lock().await = watcher;

        {
            let mut row_counts = self.inner.row_counts.lock().await;
            row_counts.retain(|path, _| snapshot.jobs_by_path.contains_key(path));
        }

        for path in watched_paths {
            let _ = self.inner.event_tx.send(path);
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        *self.inner.watcher.lock().await = None;

        if let Ok(mut slot) = self.inner.event_task.lock() {
            if let Some(task) = slot.take() {
                task.abort();
            }
        }

        if let Ok(mut slot) = self.inner.uploader_task.lock() {
            if let Some(task) = slot.take() {
                task.abort();
            }
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
            return Ok(());
        }

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
            self.inner
                .observation_tx
                .send(QueuedObservation {
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
}
