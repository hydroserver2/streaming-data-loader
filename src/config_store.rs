use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

use crate::models::{
    AppConfig, AppStateFile, ColumnMapping, FileConfig, JobConfig, JobCursor, JobLogEntry,
    JobUpsertRequest, PersistedDatasource, ServerConfig, WorkspaceStateFile,
};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct ConfigStore {
    config_dir: PathBuf,
    config_path: PathBuf,
    legacy_state_path: PathBuf,
    workspace_dir: PathBuf,
    lock: Mutex<()>,
}

impl ConfigStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            config_path: config_dir.join("config.json"),
            legacy_state_path: config_dir.join("state.json"),
            workspace_dir: config_dir.join("workspaces"),
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

    pub fn load_with_datasources(
        &self,
    ) -> Result<(ServerConfig, Vec<PersistedDatasource>), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())?;
        self.ensure_locked()?;
        let config = self.read_config_locked()?;
        let datasources = self
            .load_workspace_locked(&config.server.workspace_id)?
            .map(|w| w.datasources)
            .unwrap_or_default();
        Ok((config.server, datasources))
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
                Some(datasource.recent_logs.clone()),
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
        Ok(self
            .get_persisted_datasource(job_id)?
            .map(|datasource| {
                let count = datasource.recent_logs.len();
                datasource
                    .recent_logs
                    .into_iter()
                    .skip(count.saturating_sub(limit))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn update_cursor(&self, job_id: &str, cursor: JobCursor) -> Result<JobCursor, String> {
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
            datasource.last_pushed_timestamp = cursor.last_pushed_timestamp;
            datasource.last_pushed_row_index = cursor.last_pushed_row_index;
            datasource.last_run_at = cursor.last_run_at;
            datasource.last_error = cursor.last_error.clone();
            self.write_workspace_locked(&workspace)?;
            return Ok(cursor);
        }

        Ok(cursor)
    }

    pub fn append_log(&self, job_id: &str, entry: JobLogEntry) -> Result<JobLogEntry, String> {
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
            datasource.recent_logs.push(entry.clone());
            if datasource.recent_logs.len() > 50 {
                let keep_from = datasource.recent_logs.len() - 50;
                datasource.recent_logs = datasource.recent_logs.split_off(keep_from);
            }
            self.write_workspace_locked(&workspace)?;
            return Ok(entry);
        }

        Ok(entry)
    }

    pub fn delete_job_runtime(&self, job_id: &str) -> Result<(), String> {
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
            datasource.last_pushed_timestamp = None;
            datasource.last_pushed_row_index = None;
            datasource.last_run_at = None;
            datasource.last_error = None;
            datasource.recent_logs.clear();
            self.write_workspace_locked(&workspace)?;
            break;
        }

        Ok(())
    }

    fn ensure_locked(&self) -> Result<(), String> {
        fs::create_dir_all(&self.config_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.workspace_dir).map_err(|err| err.to_string())?;

        if !self.config_path.exists() {
            self.write_config_locked(&AppConfig::default())?;
        }

        self.migrate_legacy_workspace_data_locked()
    }

    fn read_config_locked(&self) -> Result<AppConfig, String> {
        if !self.config_path.exists() {
            return Ok(AppConfig::default());
        }

        let contents = fs::read_to_string(&self.config_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&contents).map_err(|err| err.to_string())?;

        let version = value.get("version").and_then(Value::as_u64).unwrap_or(1) as u32;
        let server = value
            .get("server")
            .cloned()
            .map(parse_server_config)
            .transpose()?
            .unwrap_or_default();
        let jobs = value
            .get("jobs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .cloned()
                    .map(parse_job_config)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        Ok(AppConfig {
            version,
            server,
            jobs,
        })
    }

    fn write_config_locked(&self, config: &AppConfig) -> Result<(), String> {
        let payload = json!({
            "version": config.version,
            "server": config.server.clone().normalized(),
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
        let value: Value = serde_json::from_str(&contents).map_err(|err| err.to_string())?;
        parse_workspace_state(value).map(Some)
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

    fn migrate_legacy_workspace_data_locked(&self) -> Result<(), String> {
        let config = self.read_config_locked()?;
        let workspace_id = config.server.workspace_id.trim().to_string();
        if workspace_id.is_empty() {
            return Ok(());
        }

        let legacy_jobs = config.jobs.clone();
        let legacy_state = self.read_legacy_state_locked()?;
        if legacy_jobs.is_empty() && legacy_state.is_none() {
            return Ok(());
        }

        let path = self.workspace_path(&workspace_id);
        if path.exists() {
            if !legacy_jobs.is_empty() {
                let stripped_config = AppConfig {
                    version: config.version,
                    server: config.server,
                    jobs: Vec::new(),
                };
                self.write_config_locked(&stripped_config)?;
            }
            return Ok(());
        }

        let workspace = WorkspaceStateFile {
            version: 1,
            workspace_id: workspace_id.clone(),
            workspace_name: String::new(),
            hydroserver_url: config.server.url.clone(),
            datasources: legacy_jobs
                .into_iter()
                .map(|job| {
                    let cursor = legacy_state
                        .as_ref()
                        .and_then(|state| state.cursors.get(&job.id).cloned());
                    let recent_logs = legacy_state
                        .as_ref()
                        .and_then(|state| state.logs.get(&job.id).cloned());
                    PersistedDatasource::from_job(job, cursor, recent_logs)
                })
                .collect(),
        };

        self.write_workspace_locked(&workspace)?;

        let stripped_config = AppConfig {
            version: config.version,
            server: config.server,
            jobs: Vec::new(),
        };
        self.write_config_locked(&stripped_config)
    }

    fn read_legacy_state_locked(&self) -> Result<Option<AppStateFile>, String> {
        if !self.legacy_state_path.exists() {
            return Ok(None);
        }

        let contents =
            fs::read_to_string(&self.legacy_state_path).map_err(|err| err.to_string())?;
        let state: AppStateFile = serde_json::from_str(&contents).map_err(|err| err.to_string())?;
        if state.cursors.is_empty() && state.logs.is_empty() {
            return Ok(None);
        }

        Ok(Some(state))
    }
}

fn parse_server_config(value: Value) -> Result<ServerConfig, String> {
    let server: ServerConfig = serde_json::from_value(value).map_err(|err| err.to_string())?;
    Ok(server.normalized())
}

fn parse_job_config(value: Value) -> Result<JobConfig, String> {
    let mut job: JobConfig =
        serde_json::from_value(normalize_job_value(value)).map_err(|err| err.to_string())?;
    job = job.normalized()?;
    Ok(job)
}

fn parse_workspace_state(value: Value) -> Result<WorkspaceStateFile, String> {
    let version = value.get("version").and_then(Value::as_u64).unwrap_or(1) as u32;
    let workspace_id = value
        .get("workspace_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let workspace_name = value
        .get("workspace_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let hydroserver_url = value
        .get("hydroserver_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let datasources = value
        .get("datasources")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .cloned()
                .map(parse_persisted_datasource)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(WorkspaceStateFile {
        version,
        workspace_id,
        workspace_name,
        hydroserver_url,
        datasources,
    })
}

fn parse_persisted_datasource(value: Value) -> Result<PersistedDatasource, String> {
    let mut datasource: PersistedDatasource =
        serde_json::from_value(normalize_job_value(value)).map_err(|err| err.to_string())?;

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

fn normalize_job_value(value: Value) -> Value {
    let mut value = value;
    if let Some(object) = value.as_object_mut() {
        if let Some(file_config) = object.get("file_config").cloned() {
            object.insert(
                "file_config".to_string(),
                migrate_file_config_value(file_config),
            );
        }
        if let Some(column_mappings) = object.get("column_mappings").cloned() {
            object.insert(
                "column_mappings".to_string(),
                normalize_column_mappings_value(column_mappings),
            );
        }
    }
    value
}

fn normalize_column_mappings_value(value: Value) -> Value {
    let Value::Array(items) = value else {
        return Value::Array(Vec::new());
    };

    Value::Array(
        items
            .into_iter()
            .filter_map(|item| match serde_json::from_value::<ColumnMapping>(item) {
                Ok(mapping) => Some(serde_json::to_value(mapping.normalized().ok()?).ok()?),
                Err(_) => None,
            })
            .collect(),
    )
}

fn migrate_file_config_value(value: Value) -> Value {
    let Some(object) = value.as_object() else {
        return serde_json::to_value(FileConfig::default()).unwrap_or(Value::Null);
    };

    if object.contains_key("timestamp")
        || object.contains_key("identifierType")
        || object.contains_key("identifier_type")
    {
        return value;
    }

    let legacy_key = string_field(object, &["timestamp_column", "timestampColumn"])
        .unwrap_or_else(|| "timestamp".to_string());
    let legacy_format = string_field(object, &["timestamp_format", "timestampFormat"]);
    let legacy_timezone = string_field(object, &["timezone"]);

    let mut timestamp = json!({
        "key": legacy_key,
    });

    if let Some(format) = legacy_format {
        timestamp["format"] = Value::String("custom".to_string());
        timestamp["customFormat"] = Value::String(format);
    } else {
        timestamp["format"] = Value::String("ISO8601".to_string());
    }

    match legacy_timezone {
        Some(timezone) if timezone.contains('/') => {
            timestamp["timezoneMode"] = Value::String("daylightSavings".to_string());
            timestamp["timezone"] = Value::String(timezone);
            if timestamp["format"] == Value::String("ISO8601".to_string()) {
                timestamp["format"] = Value::String("naive".to_string());
            }
        }
        Some(timezone) if timezone.eq_ignore_ascii_case("UTC") => {
            timestamp["timezoneMode"] = Value::String("utc".to_string());
            if timestamp["format"] == Value::String("ISO8601".to_string()) {
                timestamp["format"] = Value::String("naive".to_string());
            }
        }
        Some(timezone) => {
            timestamp["timezoneMode"] = Value::String("fixedOffset".to_string());
            timestamp["timezone"] = Value::String(timezone);
            if timestamp["format"] == Value::String("ISO8601".to_string()) {
                timestamp["format"] = Value::String("naive".to_string());
            }
        }
        None => {
            timestamp["timezoneMode"] = Value::String("embeddedOffset".to_string());
        }
    }

    json!({
        "headerRow": object
            .get("headerRow")
            .cloned()
            .or_else(|| object.get("header_row").cloned())
            .unwrap_or(Value::from(1)),
        "dataStartRow": object
            .get("dataStartRow")
            .cloned()
            .or_else(|| object.get("data_start_row").cloned())
            .unwrap_or(Value::from(2)),
        "delimiter": object
            .get("delimiter")
            .cloned()
            .unwrap_or_else(|| Value::String(",".to_string())),
        "identifierType": object
            .get("identifierType")
            .cloned()
            .or_else(|| object.get("identifier_type").cloned())
            .unwrap_or_else(|| Value::String("name".to_string())),
        "timestamp": timestamp,
    })
}

fn string_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
