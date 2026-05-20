use std::fs;

use serde_json::json;

use crate::models::{AppConfig, ServerConfig};

use super::ConfigStore;

impl ConfigStore {
    pub fn ensure(&self) -> Result<(), String> {
        let _guard = self.lock_guard()?;
        self.ensure_locked()
    }

    pub fn load(&self) -> Result<AppConfig, String> {
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
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
        let _guard = self.lock_guard()?;
        self.ensure_locked()?;
        let mut config = self.read_config_locked()?;
        config.server = ServerConfig::default();
        self.write_config_locked(&config)?;
        config.jobs.clear();
        Ok(config)
    }

    pub(super) fn ensure_locked(&self) -> Result<(), String> {
        fs::create_dir_all(&self.config_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.workspace_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.logs_dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.job_logs_dir).map_err(|err| err.to_string())?;

        if !self.config_path.exists() {
            self.write_config_locked(&AppConfig::default())?;
        }

        Ok(())
    }

    pub(super) fn read_config_locked(&self) -> Result<AppConfig, String> {
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

    pub(super) fn write_config_locked(&self, config: &AppConfig) -> Result<(), String> {
        let payload = json!({
            "version": config.version,
            "server": config.server.clone().normalized(),
            "launch_at_login_initialized": config.launch_at_login_initialized,
        });
        super::json_file::write_json_file(&self.config_path, &payload)
    }
}
