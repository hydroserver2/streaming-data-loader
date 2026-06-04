use crate::models::{JobConfig, JobUpsertRequest, PersistedDatasource, ServerConfig};

use super::{ids::generate_job_id, ConfigStore};

impl ConfigStore {
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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

    pub(super) fn active_jobs_locked(
        &self,
        server: &ServerConfig,
    ) -> Result<Vec<JobConfig>, String> {
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

pub(super) fn normalize_persisted_datasource(
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
