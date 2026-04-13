from __future__ import annotations

import json
import threading
from pathlib import Path
from uuid import uuid4

from sidecar.api.models import (
    AppConfig,
    AppStateFile,
    JobConfig,
    JobCursor,
    JobLogEntry,
    JobUpsertRequest,
    PersistedDatasource,
    ServerConfig,
    ServerConfigUpdate,
    WorkspaceStateFile,
)


class ConfigStore:
    def __init__(self, config_dir: Path) -> None:
        self.config_dir = config_dir
        self.config_path = config_dir / "config.json"
        self.legacy_state_path = config_dir / "state.json"
        self.workspace_dir = config_dir / "workspaces"
        self._lock = threading.RLock()

    def ensure(self) -> None:
        with self._lock:
            self.config_dir.mkdir(parents=True, exist_ok=True)
            self.workspace_dir.mkdir(parents=True, exist_ok=True)
            if not self.config_path.exists():
                self._write_config(AppConfig())
            self._migrate_legacy_workspace_data()

    def load(self) -> AppConfig:
        self.ensure()
        with self._lock:
            config = self._read_config()
            config.jobs = self._active_jobs(config.server)
            return config

    def save(self, config: AppConfig) -> AppConfig:
        self.ensure()
        with self._lock:
            self._write_config(config)
            self._save_active_workspace_jobs(
                server=config.server,
                jobs=config.jobs,
            )
            config.jobs = self._active_jobs(config.server)
            return config

    def set_server(
        self,
        server: ServerConfig,
        *,
        workspace_name: str = "",
    ) -> AppConfig:
        self.ensure()
        with self._lock:
            config = self._read_config()
            config.server = ServerConfig.model_validate(server)
            self._write_config(config)
            self._ensure_workspace_file(
                workspace_id=config.server.workspace_id,
                workspace_name=workspace_name,
                hydroserver_url=config.server.url,
            )
            config.jobs = self._active_jobs(config.server)
            return config

    def update_server(
        self,
        update: ServerConfigUpdate,
        *,
        workspace_name: str = "",
    ) -> AppConfig:
        return self.set_server(
            ServerConfig(
                auth_type=update.auth_type,
                url=update.url.strip(),
                api_key=update.api_key.strip() if update.auth_type == "apikey" else "",
                username=update.username.strip()
                if update.auth_type == "userpass"
                else "",
                password=update.password.strip()
                if update.auth_type == "userpass"
                else "",
                workspace_id=update.workspace_id.strip(),
            ),
            workspace_name=workspace_name,
        )

    def clear_server(self) -> AppConfig:
        self.ensure()
        with self._lock:
            config = self._read_config()
            config.server = ServerConfig()
            self._write_config(config)
            config.jobs = []
            return config

    def list_jobs(self) -> list[JobConfig]:
        return self.load().jobs

    def get_job(self, job_id: str) -> JobConfig | None:
        datasource = self.get_persisted_datasource(job_id)
        return datasource.to_job_config() if datasource else None

    def get_persisted_datasource(self, job_id: str) -> PersistedDatasource | None:
        with self._lock:
            workspace = self._load_active_workspace()
            if workspace is None:
                return None
            return next(
                (item for item in workspace.datasources if item.id == job_id),
                None,
            )

    def create_job(self, request: JobUpsertRequest) -> JobConfig:
        with self._lock:
            workspace = self._require_active_workspace()
            job = JobConfig(id=str(uuid4()), **request.model_dump())
            workspace.datasources.append(PersistedDatasource.from_job(job))
            self._write_workspace(workspace)
            return job

    def update_job(self, job_id: str, request: JobUpsertRequest) -> JobConfig | None:
        with self._lock:
            workspace = self._require_active_workspace()
            for index, existing in enumerate(workspace.datasources):
                if existing.id != job_id:
                    continue
                updated_job = JobConfig(id=job_id, **request.model_dump())
                workspace.datasources[index] = PersistedDatasource.from_job(
                    updated_job,
                    cursor=existing.to_cursor(),
                    recent_logs=existing.recent_logs,
                )
                self._write_workspace(workspace)
                return updated_job
            return None

    def delete_job(self, job_id: str) -> bool:
        with self._lock:
            workspace = self._load_active_workspace()
            if workspace is None:
                return False
            next_datasources = [
                datasource
                for datasource in workspace.datasources
                if datasource.id != job_id
            ]
            if len(next_datasources) == len(workspace.datasources):
                return False
            workspace.datasources = next_datasources
            self._write_workspace(workspace)
            return True

    def set_job_enabled(self, job_id: str, enabled: bool) -> JobConfig | None:
        with self._lock:
            workspace = self._require_active_workspace()
            for index, datasource in enumerate(workspace.datasources):
                if datasource.id != job_id:
                    continue
                workspace.datasources[index] = datasource.model_copy(
                    update={"enabled": enabled}
                )
                self._write_workspace(workspace)
                return workspace.datasources[index].to_job_config()
            return None

    def cursor_for(self, job_id: str) -> JobCursor:
        datasource = self.get_persisted_datasource(job_id)
        return datasource.to_cursor() if datasource else JobCursor()

    def logs_for(self, job_id: str, limit: int = 50) -> list[JobLogEntry]:
        datasource = self.get_persisted_datasource(job_id)
        if datasource is None:
            return []
        return datasource.recent_logs[-limit:]

    def update_cursor(self, job_id: str, cursor: JobCursor) -> JobCursor:
        with self._lock:
            workspace = self._require_active_workspace()
            for index, datasource in enumerate(workspace.datasources):
                if datasource.id != job_id:
                    continue
                workspace.datasources[index] = datasource.model_copy(
                    update=cursor.model_dump()
                )
                self._write_workspace(workspace)
                return cursor
            return cursor

    def append_log(self, job_id: str, entry: JobLogEntry) -> JobLogEntry:
        with self._lock:
            workspace = self._require_active_workspace()
            for index, datasource in enumerate(workspace.datasources):
                if datasource.id != job_id:
                    continue
                next_logs = [*datasource.recent_logs, entry][-50:]
                workspace.datasources[index] = datasource.model_copy(
                    update={"recent_logs": next_logs}
                )
                self._write_workspace(workspace)
                return entry
            return entry

    def delete_job_runtime(self, job_id: str) -> None:
        with self._lock:
            workspace = self._load_active_workspace()
            if workspace is None:
                return
            for index, datasource in enumerate(workspace.datasources):
                if datasource.id != job_id:
                    continue
                workspace.datasources[index] = datasource.model_copy(
                    update={
                        "last_pushed_timestamp": None,
                        "last_pushed_row_index": None,
                        "last_run_at": None,
                        "last_error": None,
                        "recent_logs": [],
                    }
                )
                self._write_workspace(workspace)
                return

    def _read_config(self) -> AppConfig:
        return AppConfig.model_validate_json(
            self.config_path.read_text(encoding="utf-8")
        )

    def _write_config(self, config: AppConfig) -> None:
        payload = json.dumps(
            {
                "version": config.version,
                "server": config.server.model_dump(mode="json"),
            },
            indent=2,
        )
        self.config_path.write_text(f"{payload}\n", encoding="utf-8")

    def _workspace_path(self, workspace_id: str) -> Path:
        return self.workspace_dir / f"{workspace_id}.json"

    def _ensure_workspace_file(
        self,
        *,
        workspace_id: str,
        workspace_name: str = "",
        hydroserver_url: str = "",
    ) -> WorkspaceStateFile | None:
        workspace_id = workspace_id.strip()
        if not workspace_id:
            return None

        path = self._workspace_path(workspace_id)
        if path.exists():
            workspace = WorkspaceStateFile.model_validate_json(
                path.read_text(encoding="utf-8")
            )
            updates: dict[str, str] = {}
            if workspace_name and workspace.workspace_name != workspace_name:
                updates["workspace_name"] = workspace_name
            if hydroserver_url and workspace.hydroserver_url != hydroserver_url:
                updates["hydroserver_url"] = hydroserver_url
            if updates:
                workspace = workspace.model_copy(update=updates)
                self._write_workspace(workspace)
            return workspace

        workspace = WorkspaceStateFile(
            workspace_id=workspace_id,
            workspace_name=workspace_name,
            hydroserver_url=hydroserver_url,
        )
        self._write_workspace(workspace)
        return workspace

    def _load_workspace(self, workspace_id: str) -> WorkspaceStateFile | None:
        workspace_id = workspace_id.strip()
        if not workspace_id:
            return None
        path = self._workspace_path(workspace_id)
        if not path.exists():
            return None
        return WorkspaceStateFile.model_validate_json(path.read_text(encoding="utf-8"))

    def _load_active_workspace(self) -> WorkspaceStateFile | None:
        config = self._read_config()
        return self._load_workspace(config.server.workspace_id)

    def _require_active_workspace(self) -> WorkspaceStateFile:
        config = self._read_config()
        workspace = self._ensure_workspace_file(
            workspace_id=config.server.workspace_id,
            hydroserver_url=config.server.url,
        )
        if workspace is None:
            raise RuntimeError("No active workspace is configured.")
        return workspace

    def _write_workspace(self, workspace: WorkspaceStateFile) -> None:
        payload = json.dumps(workspace.model_dump(mode="json", by_alias=True), indent=2)
        self._workspace_path(workspace.workspace_id).write_text(
            f"{payload}\n", encoding="utf-8"
        )

    def _active_jobs(self, server: ServerConfig) -> list[JobConfig]:
        workspace = self._load_workspace(server.workspace_id)
        if workspace is None:
            return []
        return [datasource.to_job_config() for datasource in workspace.datasources]

    def _save_active_workspace_jobs(
        self,
        *,
        server: ServerConfig,
        jobs: list[JobConfig],
    ) -> None:
        workspace_id = server.workspace_id.strip()
        if not workspace_id:
            return

        workspace = self._ensure_workspace_file(
            workspace_id=workspace_id,
            hydroserver_url=server.url,
        )
        if workspace is None:
            return

        existing_by_id = {datasource.id: datasource for datasource in workspace.datasources}
        next_datasources: list[PersistedDatasource] = []
        for job in jobs:
            existing = existing_by_id.get(job.id)
            next_datasources.append(
                PersistedDatasource.from_job(
                    job,
                    cursor=existing.to_cursor() if existing else None,
                    recent_logs=existing.recent_logs if existing else None,
                )
            )
        workspace.datasources = next_datasources
        self._write_workspace(workspace)

    def _migrate_legacy_workspace_data(self) -> None:
        config = self._read_config()
        workspace_id = config.server.workspace_id.strip()
        if not workspace_id:
            return

        legacy_jobs = config.jobs
        legacy_state = self._read_legacy_state()
        if not legacy_jobs and legacy_state is None:
            return

        path = self._workspace_path(workspace_id)
        if path.exists():
            if legacy_jobs:
                self._write_config(config.model_copy(update={"jobs": []}))
            return

        workspace = WorkspaceStateFile(
            workspace_id=workspace_id,
            hydroserver_url=config.server.url,
            datasources=[
                PersistedDatasource.from_job(
                    job,
                    cursor=(legacy_state.cursors.get(job.id) if legacy_state else None),
                    recent_logs=(legacy_state.logs.get(job.id) if legacy_state else None),
                )
                for job in legacy_jobs
            ],
        )
        self._write_workspace(workspace)
        self._write_config(config.model_copy(update={"jobs": []}))

    def _read_legacy_state(self) -> AppStateFile | None:
        if not self.legacy_state_path.exists():
            return None
        state = AppStateFile.model_validate_json(
            self.legacy_state_path.read_text(encoding="utf-8")
        )
        if not state.cursors and not state.logs:
            return None
        return state
