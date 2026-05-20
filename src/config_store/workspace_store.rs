use std::{fs, path::PathBuf};

use crate::models::WorkspaceStateFile;

use super::{jobs::normalize_persisted_datasource, ConfigStore};

impl ConfigStore {
    pub(super) fn workspace_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir.join(format!("{workspace_id}.json"))
    }

    pub(super) fn ensure_workspace_file_locked(
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

    pub(super) fn load_workspace_locked(
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

    pub(super) fn load_active_workspace_locked(
        &self,
    ) -> Result<Option<WorkspaceStateFile>, String> {
        let config = self.read_config_locked()?;
        self.load_workspace_locked(&config.server.workspace_id)
    }

    pub(super) fn require_active_workspace_locked(&self) -> Result<WorkspaceStateFile, String> {
        let config = self.read_config_locked()?;
        self.ensure_workspace_file_locked(&config.server.workspace_id, "", &config.server.url)?
            .ok_or_else(|| "No active workspace is configured.".to_string())
    }

    pub(super) fn write_workspace_locked(
        &self,
        workspace: &WorkspaceStateFile,
    ) -> Result<(), String> {
        let path = self.workspace_path(&workspace.workspace_id);
        let payload = serde_json::to_value(workspace).map_err(|err| err.to_string())?;
        super::json_file::write_json_file(&path, &payload)
    }
}
