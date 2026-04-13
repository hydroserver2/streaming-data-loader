use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{
    config_store::ConfigStore,
    hydroserver::{HydroServerService, ObservationPayloadRow},
    models::{
        ActionResponse, AppConfig, ConnectionState, ConnectionStatus, HealthResponse,
        IdentifierType, JobConfig, JobCursor, JobDetail, JobLogEntry, JobStatus, JobStatusSummary,
        LogLevel, ServerConfig,
    },
    timestamp::parse_timestamp_to_utc,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const JOB_CHUNK_SIZE: usize = 5000;
const SCHEDULER_POLL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct ObservationCandidate {
    timestamp: DateTime<Utc>,
    row_index: u64,
    value: Value,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    settings: AppSettings,
    config_store: ConfigStore,
    hydroserver: HydroServerService,
    running_jobs: Mutex<HashSet<String>>,
    shutdown: AtomicBool,
    scheduler: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub version: String,
    pub config_dir: PathBuf,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Result<Self, String> {
        Ok(Self {
            inner: Arc::new(AppStateInner {
                settings: AppSettings {
                    version: APP_VERSION.to_string(),
                    config_dir: config_dir.clone(),
                },
                config_store: ConfigStore::new(config_dir),
                hydroserver: HydroServerService::new()?,
                running_jobs: Mutex::new(HashSet::new()),
                shutdown: AtomicBool::new(false),
                scheduler: Mutex::new(None),
            }),
        })
    }

    pub fn initialize(&self) -> Result<(), String> {
        self.inner.config_store.ensure()?;
        self.start_scheduler();
        Ok(())
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.inner.scheduler.lock() {
            if let Some(join_handle) = handle.take() {
                let _ = join_handle.join();
            }
        }
    }

    pub fn health(&self) -> Result<HealthResponse, String> {
        let config = self.inner.config_store.load()?;
        Ok(HealthResponse {
            status: "ok".to_string(),
            version: self.inner.settings.version.clone(),
            config_dir: self.inner.settings.config_dir.to_string_lossy().to_string(),
            server_configured: config.server.is_configured(),
            connection: connection_status(&config.server),
        })
    }

    pub fn config(&self) -> Result<AppConfig, String> {
        self.inner.config_store.load()
    }

    pub fn config_store(&self) -> &ConfigStore {
        &self.inner.config_store
    }

    pub fn hydroserver(&self) -> &HydroServerService {
        &self.inner.hydroserver
    }

    pub fn is_running(&self, job_id: &str) -> bool {
        self.inner
            .running_jobs
            .lock()
            .map(|jobs| jobs.contains(job_id))
            .unwrap_or(false)
    }

    pub fn build_job_summary(&self, job: &JobConfig) -> Result<JobStatusSummary, String> {
        let cursor = self.inner.config_store.cursor_for(&job.id)?;
        let (status, status_message) = derive_job_status(job, &cursor, self.is_running(&job.id));
        Ok(JobStatusSummary {
            id: job.id.clone(),
            name: job.name.clone(),
            enabled: job.enabled,
            file_path: job.file_path.clone(),
            schedule_minutes: job.schedule_minutes,
            file_config: job.file_config.clone(),
            column_mappings: job.column_mappings.clone(),
            status,
            status_message,
            last_pushed_timestamp: cursor.last_pushed_timestamp,
            last_run_at: cursor.last_run_at,
            last_error: cursor.last_error,
        })
    }

    pub fn build_job_detail(&self, job: &JobConfig) -> Result<JobDetail, String> {
        let summary = self.build_job_summary(job)?;
        Ok(JobDetail {
            id: summary.id,
            name: summary.name,
            enabled: summary.enabled,
            file_path: summary.file_path,
            schedule_minutes: summary.schedule_minutes,
            file_config: summary.file_config,
            column_mappings: summary.column_mappings,
            status: summary.status,
            status_message: summary.status_message,
            last_pushed_timestamp: summary.last_pushed_timestamp,
            last_run_at: summary.last_run_at,
            last_error: summary.last_error,
            recent_logs: self.inner.config_store.logs_for(&job.id, 50)?,
        })
    }

    pub fn start_job(&self, job_id: &str, manual: bool) -> Result<bool, String> {
        let Some(job) = self.inner.config_store.get_job(job_id)? else {
            return Err("That job could not be found.".to_string());
        };

        if !self.mark_job_running(job_id) {
            return Ok(false);
        }

        let state = self.clone();
        let job_id = job.id.clone();
        tauri::async_runtime::spawn(async move {
            state.execute_job(job_id, manual).await;
        });
        Ok(true)
    }

    pub fn run_job_now(&self, job_id: &str) -> Result<ActionResponse, String> {
        if self.start_job(job_id, true)? {
            self.append_log(job_id, "Manual run started", LogLevel::Info)?;
            Ok(ActionResponse {
                ok: true,
                message: "Job started.".to_string(),
            })
        } else {
            Ok(ActionResponse {
                ok: true,
                message: "Job is already running.".to_string(),
            })
        }
    }

    pub fn append_log(
        &self,
        job_id: &str,
        message: &str,
        level: LogLevel,
    ) -> Result<JobLogEntry, String> {
        let entry = JobLogEntry {
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
        };
        self.inner.config_store.append_log(job_id, entry)
    }

    pub fn due_job_ids(&self) -> Result<Vec<String>, String> {
        let (server, datasources) = self.inner.config_store.load_with_datasources()?;
        if !server.is_configured() {
            return Ok(Vec::new());
        }

        let now = Utc::now();
        Ok(datasources
            .into_iter()
            .filter(|ds| ds.enabled && !self.is_running(&ds.id))
            .filter(|ds| {
                ds.last_run_at
                    .map(|t| {
                        now.signed_duration_since(t).num_minutes() >= ds.schedule_minutes as i64
                    })
                    .unwrap_or(true)
            })
            .map(|ds| ds.id)
            .collect())
    }

    fn mark_job_running(&self, job_id: &str) -> bool {
        self.inner
            .running_jobs
            .lock()
            .map(|mut jobs| jobs.insert(job_id.to_string()))
            .unwrap_or(false)
    }

    fn clear_job_running(&self, job_id: &str) {
        if let Ok(mut jobs) = self.inner.running_jobs.lock() {
            jobs.remove(job_id);
        }
    }

    fn start_scheduler(&self) {
        let mut scheduler = match self.inner.scheduler.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if scheduler.is_some() {
            return;
        }

        let state = self.clone();
        *scheduler = Some(thread::spawn(move || {
            while !state.inner.shutdown.load(Ordering::Relaxed) {
                if let Ok(job_ids) = state.due_job_ids() {
                    for job_id in job_ids {
                        let _ = state.start_job(&job_id, false);
                    }
                }

                for _ in 0..SCHEDULER_POLL_INTERVAL.as_secs() {
                    if state.inner.shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }));
    }

    async fn execute_job(&self, job_id: String, manual: bool) {
        let result = self.execute_job_inner(&job_id, manual).await;
        if let Err(error) = result {
            let now = Utc::now();
            if let Ok(existing_cursor) = self.inner.config_store.cursor_for(&job_id) {
                let _ = self.inner.config_store.update_cursor(
                    &job_id,
                    JobCursor {
                        last_run_at: Some(now),
                        last_error: Some(error.clone()),
                        ..existing_cursor
                    },
                );
            }
            let _ = self.append_log(&job_id, &error, LogLevel::Error);
        }
        self.clear_job_running(&job_id);
    }

    async fn execute_job_inner(&self, job_id: &str, manual: bool) -> Result<(), String> {
        let config = self.inner.config_store.load()?;
        let job = config
            .jobs
            .into_iter()
            .find(|job| job.id == job_id)
            .ok_or_else(|| "That job could not be found.".to_string())?;

        if !job.enabled && !manual {
            return Ok(());
        }

        if job.column_mappings.is_empty() {
            return Err("This job does not have any configured column mappings.".to_string());
        }

        if !config.server.is_configured() {
            return Err("HydroServer is not configured.".to_string());
        }

        if !manual {
            let _ = self.append_log(job_id, "Scheduled run started", LogLevel::Info);
        }

        let mut observations_by_datastream = load_job_observations(&job)?;
        let now = Utc::now();
        let existing_cursor = self.inner.config_store.cursor_for(job_id)?;

        let mut max_uploaded_timestamp = existing_cursor.last_pushed_timestamp;
        let mut max_uploaded_row_index = existing_cursor.last_pushed_row_index;
        let mut any_uploaded = false;

        for mapping in &job.column_mappings {
            let Some(candidates) = observations_by_datastream.remove(&mapping.datastream_id) else {
                continue;
            };

            let cutoff = self
                .inner
                .hydroserver
                .get_datastream_cutoff(&config.server, &mapping.datastream_id)
                .await
                .map_err(|error| {
                    normalize_datastream_lookup_error(&mapping.datastream_id, &error)
                })?;

            let filtered: Vec<ObservationCandidate> = candidates
                .into_iter()
                .filter(|candidate| {
                    cutoff
                        .map(|cutoff| candidate.timestamp > cutoff)
                        .unwrap_or(true)
                })
                .collect();

            if filtered.is_empty() {
                let _ = self.append_log(
                    job_id,
                    &format!(
                        "No new observations for {} after filtering; skipping.",
                        mapping.datastream_id
                    ),
                    LogLevel::Warning,
                );
                continue;
            }

            for chunk in filtered.chunks(JOB_CHUNK_SIZE) {
                let payload: Vec<ObservationPayloadRow> = chunk
                    .iter()
                    .map(|candidate| ObservationPayloadRow {
                        timestamp: candidate.timestamp,
                        value: candidate.value.clone(),
                    })
                    .collect();

                self.inner
                    .hydroserver
                    .post_observations(&config.server, &mapping.datastream_id, &payload)
                    .await?;
            }

            any_uploaded = true;
            if let Some(last) = filtered.last() {
                if max_uploaded_timestamp
                    .map(|current| last.timestamp > current)
                    .unwrap_or(true)
                {
                    max_uploaded_timestamp = Some(last.timestamp);
                }

                if max_uploaded_row_index
                    .map(|current| last.row_index > current)
                    .unwrap_or(true)
                {
                    max_uploaded_row_index = Some(last.row_index);
                }
            }

            let _ = self.append_log(
                job_id,
                &format!(
                    "Loaded {} observation(s) to datastream {}.",
                    filtered.len(),
                    mapping.datastream_name
                ),
                LogLevel::Info,
            );
        }

        let updated_cursor = JobCursor {
            last_run_at: Some(now),
            last_pushed_timestamp: max_uploaded_timestamp,
            last_pushed_row_index: max_uploaded_row_index,
            last_error: None,
        };
        self.inner
            .config_store
            .update_cursor(job_id, updated_cursor)?;

        if any_uploaded {
            let _ = self.append_log(job_id, "Job completed successfully.", LogLevel::Info);
        } else {
            let _ = self.append_log(
                job_id,
                "No new observations were available to load.",
                LogLevel::Info,
            );
        }

        Ok(())
    }
}

pub fn resolve_config_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(config_dir) = std::env::var("SDL_CONFIG_DIR") {
        let candidate = PathBuf::from(config_dir);
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        return Ok(candidate);
    }

    let preferred_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?;

    if try_create_dir(&preferred_dir) {
        migrate_legacy_config_dir(&preferred_dir)?;
        return Ok(preferred_dir);
    }

    if let Ok(home_dir) = app_handle.path().home_dir() {
        let fallback_dir = home_dir.join("Streaming Data Loader");
        fs::create_dir_all(&fallback_dir).map_err(|err| err.to_string())?;
        migrate_legacy_config_dir(&fallback_dir)?;
        return Ok(fallback_dir);
    }

    Err("Couldn't resolve an application data directory.".to_string())
}

fn try_create_dir(path: &Path) -> bool {
    fs::create_dir_all(path).is_ok()
}

fn migrate_legacy_config_dir(target_dir: &Path) -> Result<(), String> {
    if has_runtime_state(target_dir) {
        return Ok(());
    }

    let Some(source_dir) = legacy_config_candidates()
        .into_iter()
        .find(|candidate| candidate != target_dir && has_runtime_state(candidate))
    else {
        return Ok(());
    };

    copy_dir_contents(&source_dir, target_dir)
}

fn legacy_config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("Streaming Data Loader Data"));
    }

    if let Ok(home_dir) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        candidates.push(PathBuf::from(home_dir).join("Streaming Data Loader"));
    }

    candidates
}

fn has_runtime_state(path: &Path) -> bool {
    path.join("config.json").exists() || path.join("workspaces").is_dir()
}

fn copy_dir_contents(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|err| err.to_string())?;

    for entry in fs::read_dir(source_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_contents(&source_path, &target_path)?;
        } else if source_path.is_file() && !target_path.exists() {
            fs::copy(&source_path, &target_path).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn connection_status(server: &ServerConfig) -> ConnectionStatus {
    if !server.is_configured() {
        return ConnectionStatus {
            state: ConnectionState::NotConfigured,
            message: "HydroServer not configured".to_string(),
        };
    }

    ConnectionStatus {
        state: ConnectionState::Configured,
        message: "HydroServer configured".to_string(),
    }
}

fn derive_job_status(job: &JobConfig, cursor: &JobCursor, is_running: bool) -> (JobStatus, String) {
    if is_running {
        return (JobStatus::Running, "Running now".to_string());
    }
    if !job.enabled {
        return (JobStatus::Disabled, "Paused".to_string());
    }
    if let Some(last_error) = &cursor.last_error {
        return (JobStatus::Error, last_error.clone());
    }
    if cursor.last_pushed_timestamp.is_none() {
        return (JobStatus::Pending, "Ready for the first run".to_string());
    }
    if let Some(last_run_at) = cursor.last_run_at {
        let stale_after_minutes = i64::max((job.schedule_minutes * 2) as i64, 1);
        if Utc::now().signed_duration_since(last_run_at).num_minutes() > stale_after_minutes {
            return (JobStatus::Warning, "This job looks stale".to_string());
        }
    }
    (JobStatus::Healthy, "Last push succeeded".to_string())
}

fn load_job_observations(
    job: &JobConfig,
) -> Result<HashMap<String, Vec<ObservationCandidate>>, String> {
    let csv_text = read_csv_text(&job.file_path)?;
    let delimiter = job
        .file_config
        .delimiter
        .chars()
        .next()
        .ok_or_else(|| "Delimiter is required.".to_string())?;
    let rows = read_csv_rows(&csv_text, delimiter)?;
    if rows.is_empty() {
        return Ok(HashMap::new());
    }

    let data_start_index = job.file_config.data_start_row.saturating_sub(1) as usize;
    let timestamp_index = resolve_column_index(&rows, job, &job.file_config.timestamp.key)?;
    let mapping_indexes = job
        .column_mappings
        .iter()
        .map(|mapping| {
            resolve_column_index(&rows, job, &mapping.csv_column)
                .map(|index| (mapping.datastream_id.clone(), index))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut observations_by_datastream: HashMap<String, Vec<ObservationCandidate>> = HashMap::new();

    for (row_number, row) in rows.iter().enumerate().skip(data_start_index) {
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

        for (datastream_id, column_index) in &mapping_indexes {
            let value = row
                .get(*column_index)
                .map(String::as_str)
                .unwrap_or_default()
                .trim();
            if value.is_empty() {
                continue;
            }

            observations_by_datastream
                .entry(datastream_id.clone())
                .or_default()
                .push(ObservationCandidate {
                    timestamp,
                    row_index: csv_row_number,
                    value: parse_observation_value(value),
                });
        }
    }

    Ok(observations_by_datastream)
}

fn resolve_column_index(rows: &[Vec<String>], job: &JobConfig, key: &str) -> Result<usize, String> {
    if job.file_config.identifier_type == IdentifierType::Index {
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
    let header = rows
        .get(header_row.saturating_sub(1) as usize)
        .ok_or_else(|| format!("Header row {header_row} is not present in the CSV file."))?;
    let normalized_key = key.trim();

    header
        .iter()
        .enumerate()
        .find_map(|(index, value)| {
            let name = normalize_header_name(value, index);
            if name == normalized_key {
                Some(index)
            } else {
                None
            }
        })
        .ok_or_else(|| format!("Column '{key}' was not found in the CSV header."))
}

fn normalize_header_name(value: &str, index: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        format!("Column {}", index + 1)
    } else {
        trimmed.to_string()
    }
}

fn parse_observation_value(value: &str) -> Value {
    if let Ok(number) = value.parse::<i64>() {
        return json!(number);
    }
    if let Ok(number) = value.parse::<f64>() {
        return json!(number);
    }
    if let Ok(boolean) = value.parse::<bool>() {
        return json!(boolean);
    }
    Value::String(value.to_string())
}

fn read_csv_text(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    crate::csv_preview::decode_text(&bytes).map(|(text, _)| text)
}

fn read_csv_rows(csv_text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter as u8)
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    reader
        .records()
        .map(|record| {
            record
                .map(|record| record.iter().map(|value| value.to_string()).collect())
                .map_err(|err| err.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::read_csv_rows;

    #[test]
    fn read_csv_rows_allows_variable_width_preamble_rows() {
        let csv_text = "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C,SpecificConductance_uScm,DissolvedOxygen_mgL,Battery_V
2026-04-03 08:00:00,2.41,7.8,418,10.7,12.61
";

        let rows = read_csv_rows(csv_text, ',').expect("csv should parse");

        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], vec!["Station", "Example Creek at Demo Site"]);
        assert_eq!(rows[2].len(), 6);
        assert_eq!(rows[3].len(), 6);
    }
}

fn normalize_datastream_lookup_error(datastream_id: &str, error: &str) -> String {
    if error.contains("404") {
        format!(
            "The HydroServer data loader could not find a destination datastream with ID '{datastream_id}'. Ensure the HydroServer connection is configured correctly and is authorized to access the datastream."
        )
    } else {
        error.to_string()
    }
}
