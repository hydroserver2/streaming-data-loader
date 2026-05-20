use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use crate::models::JobLogEntry;

use super::ConfigStore;

const JOB_LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;
const JOB_LOG_ROTATE_FILES: usize = 7;

impl ConfigStore {
    pub fn logs_for(&self, job_id: &str, limit: usize) -> Result<Vec<JobLogEntry>, String> {
        let _guard = self.lock_guard()?;
        self.ensure_locked()?;

        self.read_job_logs_locked(job_id, limit)
    }

    pub fn append_log(&self, job_id: &str, entry: JobLogEntry) -> Result<JobLogEntry, String> {
        let _guard = self.lock_guard()?;
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

    pub fn job_log_file_path(&self, job_id: &str) -> Result<Option<PathBuf>, String> {
        let _guard = self.lock_guard()?;
        self.ensure_locked()?;
        Ok(self.job_log_paths_oldest_to_newest(job_id).pop())
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

    pub(super) fn delete_job_logs_locked(&self, job_id: &str) -> Result<(), String> {
        for path in self.job_log_paths_oldest_to_newest(job_id) {
            if path.exists() {
                fs::remove_file(path).map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }
}
