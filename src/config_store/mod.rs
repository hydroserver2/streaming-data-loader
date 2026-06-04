use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

mod app_config;
mod cursors;
mod ids;
mod job_logs;
mod jobs;
mod json_file;
mod workspace_store;

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

    pub(super) fn lock_guard(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.lock
            .lock()
            .map_err(|_| "Config lock poisoned.".to_string())
    }
}

#[cfg(test)]
#[path = "../tests/config_store.rs"]
mod tests;
