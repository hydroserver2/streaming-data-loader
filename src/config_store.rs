use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::models::{
    AppConfig, JobConfig, JobCursor, JobLogEntry, JobUpsertRequest, PersistedDatasource,
    ServerConfig, WorkspaceStateFile,
};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
const JOB_LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;
const JOB_LOG_ROTATE_FILES: usize = 7;

pub struct ConfigStore {
    config_dir: PathBuf,
    config_path: PathBuf,
    workspace_dir: PathBuf,
    logs_dir: PathBuf,
    job_logs_dir: PathBuf,
    lock: Mutex<()>,
}

impl ConfigStore {
    pub fn new(config_dir: PathBuf) -> Self {
        let logs_dir = config_dir.join("logs");
        let job_logs_dir = logs_dir.join("jobs");
        Self {
            config_path: config_dir.join("config.json"),
            workspace_dir: config_dir.join("workspaces"),
            logs_dir,
            job_logs_dir,
            config_dir,
            lock: Mutex::new(()),
        }
    }

    pub fn ensure(&self) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()
    }

    pub fn load(&self) -> Result<AppConfig, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut config = self.read_config_locked()?;
        config.jobs = self.active_jobs_locked(&config.server)?;
        Ok(config)
    }

    pub fn set_server(
        &self,
        server: ServerConfig,
        workspace_name: &str,
    ) -> Result<AppConfig, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut config = self.read_config_locked()?;
        config.server = server.normalized();
        self.write_config_locked(&config)?;
        self.ensure_workspace_file_locked(
            &config.server.workspace_id,
            workspace_name,
            &config.server.url,
        )?;
        config.jobs = self.active_jobs_locked(&config.server)?;
        Ok(config)
    }

    pub fn clear_server(&self) -> Result<AppConfig, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut config = self.read_config_locked()?;
        config.server = ServerConfig::default();
        self.write_config_locked(&config)?;
        config.jobs.clear();
        Ok(config)
    }

    pub fn list_jobs(&self) -> Result<Vec<JobConfig>, String> {
        Ok(self.load()?.jobs)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<JobConfig>, String> {
        Ok(self
            .get_persisted_datasource(job_id)?
            .map(|datasource| datasource.to_job_config()))
    }

    pub fn get_persisted_datasource(
        &self,
        job_id: &str,
    ) -> Result<Option<PersistedDatasource>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(workspace) = self.load_active_workspace_locked()? else {
            return Ok(None);
        };

        Ok(workspace
            .datasources
            .into_iter()
            .find(|item| item.id == job_id))
    }

    pub fn create_job(&self, request: JobUpsertRequest) -> Result<JobConfig, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut workspace = self.require_active_workspace_locked()?;
        let job = JobConfig::from_request(generate_job_id(), request)?;
        workspace
            .datasources
            .push(PersistedDatasource::from_job(job.clone(), None, None));
        self.write_workspace_locked(&workspace)?;
        Ok(job)
    }

    pub fn update_job(
        &self,
        job_id: &str,
        request: JobUpsertRequest,
    ) -> Result<Option<JobConfig>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut workspace = self.require_active_workspace_locked()?;

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }

            let updated_job = JobConfig::from_request(job_id.to_string(), request)?;
            *datasource = PersistedDatasource::from_job(
                updated_job.clone(),
                Some(datasource.to_cursor()),
                None,
            );
            self.write_workspace_locked(&workspace)?;
            return Ok(Some(updated_job));
        }

        Ok(None)
    }

    pub fn delete_job(&self, job_id: &str) -> Result<bool, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(false);
        };

        let original_len = workspace.datasources.len();
        workspace
            .datasources
            .retain(|datasource| datasource.id != job_id);
        if workspace.datasources.len() == original_len {
            return Ok(false);
        }

        self.write_workspace_locked(&workspace)?;
        Ok(true)
    }

    pub fn set_job_enabled(
        &self,
        job_id: &str,
        enabled: bool,
    ) -> Result<Option<JobConfig>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let mut workspace = self.require_active_workspace_locked()?;

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }
            datasource.enabled = enabled;
            let job = datasource.to_job_config();
            self.write_workspace_locked(&workspace)?;
            return Ok(Some(job));
        }

        Ok(None)
    }

    pub fn cursor_for(&self, job_id: &str) -> Result<JobCursor, String> {
        Ok(self
            .get_persisted_datasource(job_id)?
            .map(|datasource| datasource.to_cursor())
            .unwrap_or_default())
    }

    pub fn logs_for(&self, job_id: &str, limit: usize) -> Result<Vec<JobLogEntry>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;

        self.read_job_logs_locked(job_id, limit)
    }

    /// Atomically record a successful batch upload for a specific datastream.
    /// Advances the datastream's cursor, clears its error, and recomputes the
    /// job-level aggregates from the surviving datastreams.
    pub fn record_datastream_success(
        &self,
        job_id: &str,
        datastream_id: &str,
        max_row_index: u64,
        max_timestamp: DateTime<Utc>,
        last_run_at: DateTime<Utc>,
    ) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(());
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }

            let entry = datasource
                .datastream_cursors
                .entry(datastream_id.to_string())
                .or_default();
            entry.last_pushed_row_index = Some(
                entry
                    .last_pushed_row_index
                    .map(|current| current.max(max_row_index))
                    .unwrap_or(max_row_index),
            );
            entry.last_pushed_timestamp = Some(
                entry
                    .last_pushed_timestamp
                    .map(|current| current.max(max_timestamp))
                    .unwrap_or(max_timestamp),
            );
            entry.last_error = None;

            datasource.last_run_at = Some(last_run_at);
            recompute_job_aggregates(datasource);
            self.write_workspace_locked(&workspace)?;
            return Ok(());
        }

        Ok(())
    }

    /// Clear all per-datastream cursors for a job after the watched CSV was
    /// rotated or truncated. Without this, `record_datastream_success` keeps
    /// `.max()`ing against the pre-rotation high-water mark and the scanner
    /// re-queues the same rows on every tick (bug_001).
    pub fn reset_job_datastream_cursors(&self, job_id: &str) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(());
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }
            datasource.datastream_cursors.clear();
            recompute_job_aggregates(datasource);
            self.write_workspace_locked(&workspace)?;
            return Ok(());
        }

        Ok(())
    }

    /// Atomically clear the job-level `last_error` and update `last_run_at`.
    /// Used by the scanner after a successful scan iteration.  Taking the
    /// config lock for the entire read-modify-write means a concurrent
    /// `set_job_running` can't be clobbered between a separate read and write
    /// (bug_004).
    pub fn clear_last_error(&self, job_id: &str, last_run_at: DateTime<Utc>) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(());
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }

            datasource.last_error = None;
            datasource.last_run_at = Some(last_run_at);
            self.write_workspace_locked(&workspace)?;
            return Ok(());
        }

        Ok(())
    }

    /// Atomically record a failed batch upload for a specific datastream.
    /// Sets the datastream's error without advancing its cursor and
    /// recomputes the job-level aggregates.
    pub fn record_datastream_failure(
        &self,
        job_id: &str,
        datastream_id: &str,
        error_message: &str,
        last_run_at: DateTime<Utc>,
    ) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(());
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }

            let entry = datasource
                .datastream_cursors
                .entry(datastream_id.to_string())
                .or_default();
            entry.last_error = Some(error_message.to_string());

            datasource.last_run_at = Some(last_run_at);
            recompute_job_aggregates(datasource);
            self.write_workspace_locked(&workspace)?;
            return Ok(());
        }

        Ok(())
    }

    pub fn append_log(&self, job_id: &str, entry: JobLogEntry) -> Result<JobLogEntry, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let has_job = self
            .load_active_workspace_locked()?
            .map(|workspace| {
                workspace
                    .datasources
                    .into_iter()
                    .any(|item| item.id == job_id)
            })
            .unwrap_or(false);
        if has_job {
            self.append_job_log_locked(job_id, &entry)?;
        }

        Ok(entry)
    }

    pub fn set_job_running(&self, job_id: &str, is_running: bool) -> Result<bool, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            return Ok(false);
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }

            datasource.is_running = is_running;
            self.write_workspace_locked(&workspace)?;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn clear_all_running_jobs(&self) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;

        let config = self.read_config_locked()?;
        let Some(mut workspace) = self.load_workspace_locked(&config.server.workspace_id)? else {
            return Ok(());
        };

        let mut changed = false;
        for datasource in &mut workspace.datasources {
            if datasource.is_running {
                datasource.is_running = false;
                changed = true;
            }
        }

        if changed {
            self.write_workspace_locked(&workspace)?;
        }

        Ok(())
    }

    pub fn delete_job_runtime(&self, job_id: &str) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let Some(mut workspace) = self.load_active_workspace_locked()? else {
            self.delete_job_logs_locked(job_id)?;
            return Ok(());
        };

        for datasource in &mut workspace.datasources {
            if datasource.id != job_id {
                continue;
            }
            datasource.last_pushed_timestamp = None;
            datasource.last_pushed_row_index = None;
            datasource.last_run_at = None;
            datasource.last_error = None;
            datasource.is_running = false;
            datasource.datastream_cursors.clear();
            datasource.recent_logs.clear();
            self.write_workspace_locked(&workspace)?;
            break;
        }
        self.delete_job_logs_locked(job_id)?;

        Ok(())
    }

    pub fn job_log_file_path(&self, job_id: &str) -> Result<Option<PathBuf>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        Ok(self.job_log_paths_oldest_to_newest(job_id).pop())
    }

    fn ensure_locked(&self) -> Result<(), String> {
        fs::create_dir_all(&self.config_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.workspace_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.logs_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.job_logs_dir).map_err(|err| err.to_string())?;

        if !self.config_path.exists() {
            self.write_config_locked(&AppConfig::default())?;
        }

        Ok(())
    }

    fn read_config_locked(&self) -> Result<AppConfig, String> {
        if !self.config_path.exists() {
            return Ok(AppConfig::default());
        }

        let contents = fs::read_to_string(&self.config_path).map_err(|err| err.to_string())?;
        let mut config: AppConfig =
            serde_json::from_str(&contents).map_err(|err| err.to_string())?;
        config.server = config.server.normalized();
        config.jobs.clear();
        Ok(config)
    }

    fn write_config_locked(&self, config: &AppConfig) -> Result<(), String> {
        let payload = json!({
            "version": config.version,
            "server": config.server.clone().normalized(),
            "launch_at_login_initialized": config.launch_at_login_initialized,
        });
        write_json_file(&self.config_path, &payload)
    }

    fn workspace_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir.join(format!("{workspace_id}.json"))
    }

    fn ensure_workspace_file_locked(
        &self,
        workspace_id: &str,
        workspace_name: &str,
        hydroserver_url: &str,
    ) -> Result<Option<WorkspaceStateFile>, String> {
        let workspace_id = workspace_id.trim();
        if workspace_id.is_empty() {
            return Ok(None);
        }

        let path = self.workspace_path(workspace_id);
        if path.exists() {
            let mut workspace = self
                .load_workspace_locked(workspace_id)?
                .unwrap_or_default();
            let mut changed = false;

            if !workspace_name.trim().is_empty()
                && workspace.workspace_name != workspace_name.trim()
            {
                workspace.workspace_name = workspace_name.trim().to_string();
                changed = true;
            }
            if !hydroserver_url.trim().is_empty()
                && workspace.hydroserver_url != hydroserver_url.trim()
            {
                workspace.hydroserver_url = hydroserver_url.trim().to_string();
                changed = true;
            }

            if changed {
                self.write_workspace_locked(&workspace)?;
            }

            return Ok(Some(workspace));
        }

        let workspace = WorkspaceStateFile {
            version: 1,
            workspace_id: workspace_id.to_string(),
            workspace_name: workspace_name.trim().to_string(),
            hydroserver_url: hydroserver_url.trim().to_string(),
            datasources: Vec::new(),
        };
        self.write_workspace_locked(&workspace)?;
        Ok(Some(workspace))
    }

    fn load_workspace_locked(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceStateFile>, String> {
        let workspace_id = workspace_id.trim();
        if workspace_id.is_empty() {
            return Ok(None);
        }

        let path = self.workspace_path(workspace_id);
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut workspace: WorkspaceStateFile =
            serde_json::from_str(&contents).map_err(|err| err.to_string())?;
        workspace.workspace_id = workspace.workspace_id.trim().to_string();
        workspace.workspace_name = workspace.workspace_name.trim().to_string();
        workspace.hydroserver_url = workspace.hydroserver_url.trim().to_string();
        workspace.datasources = workspace
            .datasources
            .into_iter()
            .map(normalize_persisted_datasource)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(workspace))
    }

    fn load_active_workspace_locked(&self) -> Result<Option<WorkspaceStateFile>, String> {
        let config = self.read_config_locked()?;
        self.load_workspace_locked(&config.server.workspace_id)
    }

    fn require_active_workspace_locked(&self) -> Result<WorkspaceStateFile, String> {
        let config = self.read_config_locked()?;
        self.ensure_workspace_file_locked(&config.server.workspace_id, "", &config.server.url)?
            .ok_or_else(|| "No active workspace is configured.".to_string())
    }

    fn write_workspace_locked(&self, workspace: &WorkspaceStateFile) -> Result<(), String> {
        let path = self.workspace_path(&workspace.workspace_id);
        let payload = serde_json::to_value(workspace).map_err(|err| err.to_string())?;
        write_json_file(&path, &payload)
    }

    fn job_log_path(&self, job_id: &str) -> PathBuf {
        self.job_logs_dir.join(format!("{job_id}.log"))
    }

    fn rotated_job_log_path(&self, job_id: &str, index: usize) -> PathBuf {
        self.job_logs_dir.join(format!("{job_id}.{index}.log"))
    }

    fn job_log_paths_oldest_to_newest(&self, job_id: &str) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for index in (1..=JOB_LOG_ROTATE_FILES).rev() {
            let rotated = self.rotated_job_log_path(job_id, index);
            if rotated.exists() {
                paths.push(rotated);
            }
        }

        let current = self.job_log_path(job_id);
        if current.exists() {
            paths.push(current);
        }

        paths
    }

    fn append_job_log_locked(&self, job_id: &str, entry: &JobLogEntry) -> Result<(), String> {
        let payload = serde_json::to_string(entry).map_err(|err| err.to_string())?;
        let line = format!("{payload}\n");
        self.rotate_job_logs_locked(job_id, line.len() as u64)?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.job_log_path(job_id))
            .map_err(|err| err.to_string())?;
        file.write_all(line.as_bytes())
            .map_err(|err| err.to_string())
    }

    fn rotate_job_logs_locked(&self, job_id: &str, incoming_bytes: u64) -> Result<(), String> {
        let current = self.job_log_path(job_id);
        let current_len = current
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        if current_len + incoming_bytes <= JOB_LOG_ROTATE_BYTES {
            return Ok(());
        }

        let oldest = self.rotated_job_log_path(job_id, JOB_LOG_ROTATE_FILES);
        if oldest.exists() {
            fs::remove_file(&oldest).map_err(|err| err.to_string())?;
        }

        for index in (1..JOB_LOG_ROTATE_FILES).rev() {
            let source = self.rotated_job_log_path(job_id, index);
            if source.exists() {
                fs::rename(&source, self.rotated_job_log_path(job_id, index + 1))
                    .map_err(|err| err.to_string())?;
            }
        }

        if current.exists() {
            fs::rename(&current, self.rotated_job_log_path(job_id, 1))
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    fn read_job_logs_locked(&self, job_id: &str, limit: usize) -> Result<Vec<JobLogEntry>, String> {
        let mut entries = Vec::new();
        for path in self.job_log_paths_oldest_to_newest(job_id) {
            let file = fs::File::open(path).map_err(|err| err.to_string())?;
            for line in BufReader::new(file).lines() {
                let line = line.map_err(|err| err.to_string())?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<JobLogEntry>(trimmed) {
                    entries.push(entry);
                }
            }
        }

        if entries.len() > limit {
            let keep_from = entries.len() - limit;
            entries = entries.split_off(keep_from);
        }

        Ok(entries)
    }

    fn delete_job_logs_locked(&self, job_id: &str) -> Result<(), String> {
        for path in self.job_log_paths_oldest_to_newest(job_id) {
            if path.exists() {
                fs::remove_file(path).map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    fn active_jobs_locked(&self, server: &ServerConfig) -> Result<Vec<JobConfig>, String> {
        let Some(workspace) = self.load_workspace_locked(&server.workspace_id)? else {
            return Ok(Vec::new());
        };

        Ok(workspace
            .datasources
            .into_iter()
            .map(|datasource| datasource.to_job_config())
            .collect())
    }
}

fn normalize_persisted_datasource(
    mut datasource: PersistedDatasource,
) -> Result<PersistedDatasource, String> {
    let normalized_job = datasource.to_job_config().normalized()?;
    datasource.id = normalized_job.id;
    datasource.name = normalized_job.name;
    datasource.enabled = normalized_job.enabled;
    datasource.file_path = normalized_job.file_path;
    datasource.schedule_minutes = normalized_job.schedule_minutes;
    datasource.file_config = normalized_job.file_config;
    datasource.column_mappings = normalized_job.column_mappings;
    Ok(datasource)
}

/// Recomputes the job-level `last_pushed_row_index`, `last_pushed_timestamp`,
/// and `last_error` from the per-datastream cursors of the currently-configured
/// column mappings. The job-level fields are derived aggregates used for the UI
/// status display; the per-datastream cursors are authoritative for resumption.
fn recompute_job_aggregates(datasource: &mut PersistedDatasource) {
    let active_ids: Vec<&str> = datasource
        .column_mappings
        .iter()
        .map(|mapping| mapping.datastream_id.as_str())
        .collect();

    if active_ids.is_empty() {
        datasource.last_pushed_row_index = None;
        datasource.last_pushed_timestamp = None;
        datasource.last_error = None;
        return;
    }

    let mut min_row: Option<u64> = None;
    let mut min_ts: Option<DateTime<Utc>> = None;
    let mut any_missing_row = false;
    let mut any_missing_ts = false;
    let mut aggregate_error: Option<String> = None;

    for id in &active_ids {
        let cursor = datasource.datastream_cursors.get(*id);
        match cursor.and_then(|c| c.last_pushed_row_index) {
            Some(idx) => min_row = Some(min_row.map_or(idx, |current| current.min(idx))),
            None => any_missing_row = true,
        }
        match cursor.and_then(|c| c.last_pushed_timestamp) {
            Some(ts) => min_ts = Some(min_ts.map_or(ts, |current| current.min(ts))),
            None => any_missing_ts = true,
        }
        if aggregate_error.is_none() {
            if let Some(error) = cursor.and_then(|c| c.last_error.clone()) {
                aggregate_error = Some(error);
            }
        }
    }

    datasource.last_pushed_row_index = if any_missing_row { None } else { min_row };
    datasource.last_pushed_timestamp = if any_missing_ts { None } else { min_ts };
    datasource.last_error = aggregate_error;
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
    fs::write(path, format!("{payload}\n")).map_err(|err| err.to_string())
}

fn generate_job_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = JOB_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let mixed = nanos ^ (counter << 32) ^ ((std::process::id() as u128) << 64);
    let hex = format!("{mixed:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
#[path = "tests/config_store.rs"]
mod tests;
