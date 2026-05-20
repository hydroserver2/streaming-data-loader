use chrono::{DateTime, Utc};

use crate::models::{JobCursor, PersistedDatasource};

use super::ConfigStore;

impl ConfigStore {
    pub fn cursor_for(&self, job_id: &str) -> Result<JobCursor, String> {
        Ok(self
            .get_persisted_datasource(job_id)?
            .map(|datasource| datasource.to_cursor())
            .unwrap_or_default())
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
    /// Used by the scanner after a successful scan iteration. Taking the
    /// config lock for the entire read-modify-write means a concurrent
    /// `set_job_running` can't be clobbered between a separate read and write
    /// (bug_004).
    pub fn clear_last_error(&self, job_id: &str, last_run_at: DateTime<Utc>) -> Result<(), String> {
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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

    pub fn set_job_running(&self, job_id: &str, is_running: bool) -> Result<bool, String> {
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
