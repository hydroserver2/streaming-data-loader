use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::{
    config_store::ConfigStore,
    hydroserver::HydroServerService,
    models::{
        AppBootstrapResponse, AppConfig, ConnectionState, ConnectionStatus, DaemonStatusSnapshot,
        HealthResponse, JobConfig, JobCursor, JobDetail, JobLogEntry, JobStatus, JobStatusSummary,
        LogLevel, ServerConfig,
    },
    service_paths::{
        active_app_directory_name, resolve_shared_service_config_dir, APP_DIRECTORY_NAME,
        DEV_APP_DIRECTORY_NAME,
    },
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
#[cfg_attr(not(test), allow(dead_code))]
const BUNDLE_IDENTIFIER: &str = "com.streaming-data-loader";
#[cfg_attr(not(test), allow(dead_code))]
const LEGACY_BUNDLE_IDENTIFIER: &str = "com.streaming-data-loader.app";

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    settings: AppSettings,
    config_store: Arc<ConfigStore>,
    hydroserver: Arc<HydroServerService>,
}

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub version: String,
    pub config_dir: PathBuf,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Result<Self, String> {
        let config_store = Arc::new(ConfigStore::new(config_dir.clone()));
        let hydroserver = Arc::new(HydroServerService::new()?);

        Ok(Self {
            inner: Arc::new(AppStateInner {
                settings: AppSettings {
                    version: APP_VERSION.to_string(),
                    config_dir,
                },
                config_store,
                hydroserver,
            }),
        })
    }

    pub fn initialize(&self) -> Result<(), String> {
        self.inner.config_store.ensure()
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
        self.inner.config_store.as_ref()
    }

    pub fn config_store_handle(&self) -> Arc<ConfigStore> {
        self.inner.config_store.clone()
    }

    pub fn hydroserver(&self) -> &HydroServerService {
        self.inner.hydroserver.as_ref()
    }

    pub fn hydroserver_handle(&self) -> Arc<HydroServerService> {
        self.inner.hydroserver.clone()
    }

    pub fn status_snapshot(&self) -> Result<DaemonStatusSnapshot, String> {
        let health = self.health()?;
        let config = self.config()?;
        let jobs = config
            .jobs
            .iter()
            .map(|job| self.build_job_summary(job))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DaemonStatusSnapshot {
            health,
            config,
            jobs,
        })
    }

    pub fn bootstrap(&self) -> Result<AppBootstrapResponse, String> {
        let snapshot = self.status_snapshot()?;
        Ok(AppBootstrapResponse {
            health: snapshot.health,
            config: snapshot.config,
            jobs: snapshot.jobs,
        })
    }

    pub fn build_job_summary(&self, job: &JobConfig) -> Result<JobStatusSummary, String> {
        let cursor = self.inner.config_store.cursor_for(&job.id)?;
        let (status, status_message) = derive_job_status(job, &cursor);
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
            recent_logs: self.inner.config_store.logs_for(&job.id, 200)?,
        })
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
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn resolve_config_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        let preferred_dir = resolve_shared_service_config_dir()?;
        migrate_legacy_config_dir(app_handle, &preferred_dir)?;
        return Ok(preferred_dir);
    }

    let preferred_dir = preferred_user_data_dir(
        app_handle.path().app_data_dir().ok(),
        app_handle.path().home_dir().ok(),
    )?;

    migrate_legacy_config_dir(app_handle, &preferred_dir)?;

    if try_create_dir(&preferred_dir) {
        return Ok(preferred_dir);
    }

    if let Ok(home_dir) = app_handle.path().home_dir() {
        let fallback_dir = home_dir.join(active_app_directory_name());
        migrate_legacy_config_dir(app_handle, &fallback_dir)?;
        fs::create_dir_all(&fallback_dir).map_err(|err| err.to_string())?;
        return Ok(fallback_dir);
    }

    Err("Couldn't resolve an application data directory.".to_string())
}

#[cfg_attr(not(test), allow(dead_code))]
fn preferred_user_data_dir(
    app_data_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(app_data_dir) = app_data_dir {
        return Ok(if cfg!(debug_assertions) {
            app_data_dir.join("dev")
        } else {
            app_data_dir
        });
    }

    if let Some(home_dir) = home_dir {
        return Ok(home_dir.join(active_app_directory_name()));
    }

    Err("Couldn't resolve an application data directory.".to_string())
}

#[cfg_attr(not(test), allow(dead_code))]
fn try_create_dir(path: &Path) -> bool {
    fs::create_dir_all(path).is_ok()
}

#[cfg_attr(not(test), allow(dead_code))]
fn migrate_legacy_config_dir(app_handle: &AppHandle, target_dir: &Path) -> Result<(), String> {
    if has_runtime_state(target_dir) {
        return Ok(());
    }

    let Some(source_dir) = legacy_config_candidates(app_handle)
        .into_iter()
        .find(|candidate| candidate != target_dir && has_runtime_state(candidate))
    else {
        return Ok(());
    };

    move_or_copy_dir_contents(&source_dir, target_dir)
}

#[cfg_attr(not(test), allow(dead_code))]
fn legacy_config_candidates(app_handle: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(data_dir) = app_handle.path().data_dir() {
        candidates.push(data_dir.join(LEGACY_BUNDLE_IDENTIFIER));
        candidates.push(data_dir.join(BUNDLE_IDENTIFIER));
    }

    if let Ok(document_dir) = app_handle.path().document_dir() {
        candidates.push(document_dir.join(APP_DIRECTORY_NAME));
        if cfg!(debug_assertions) {
            candidates.push(document_dir.join(DEV_APP_DIRECTORY_NAME));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("Streaming Data Loader Data"));
    }

    if let Ok(home_dir) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_dir = PathBuf::from(home_dir);
        candidates.push(home_dir.join(APP_DIRECTORY_NAME));
        if cfg!(debug_assertions) {
            candidates.push(home_dir.join(DEV_APP_DIRECTORY_NAME));
        }
    }

    candidates
}

#[cfg_attr(not(test), allow(dead_code))]
fn has_runtime_state(path: &Path) -> bool {
    path.join("config.json").exists() || path.join("workspaces").is_dir()
}

#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
fn move_or_copy_dir_contents(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if !target_dir.exists() && fs::rename(source_dir, target_dir).is_ok() {
        return Ok(());
    }

    copy_dir_contents(source_dir, target_dir)
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

fn derive_job_status(job: &JobConfig, cursor: &JobCursor) -> (JobStatus, String) {
    if cursor.is_running {
        return (JobStatus::Running, "Running now".to_string());
    }
    if !job.enabled {
        return (JobStatus::Disabled, "Paused".to_string());
    }
    if let Some(last_error) = &cursor.last_error {
        return (JobStatus::Error, last_error.clone());
    }
    if cursor.last_pushed_timestamp.is_none() {
        return (JobStatus::Pending, "Watching for new rows".to_string());
    }
    (JobStatus::Healthy, "Watching for new rows".to_string())
}

#[cfg(test)]
#[path = "tests/runtime.rs"]
mod tests;
