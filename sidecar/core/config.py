from __future__ import annotations

import json
import threading
from pathlib import Path
from uuid import uuid4

from sidecar.api.models import AppConfig, JobConfig, JobUpsertRequest, ServerConfig, ServerConfigUpdate


class ConfigStore:
    def __init__(self, config_dir: Path) -> None:
        self.config_dir = config_dir
        self.config_path = config_dir / "config.json"
        self._lock = threading.Lock()

    def ensure(self) -> None:
        self.config_dir.mkdir(parents=True, exist_ok=True)
        if not self.config_path.exists():
            self._write(AppConfig())

    def load(self) -> AppConfig:
        self.ensure()
        with self._lock:
            return AppConfig.model_validate_json(self.config_path.read_text(encoding="utf-8"))

    def save(self, config: AppConfig) -> AppConfig:
        self.config_dir.mkdir(parents=True, exist_ok=True)
        self._write(config)
        return config

    def _write(self, config: AppConfig) -> None:
        payload = json.dumps(config.model_dump(mode="json"), indent=2)
        with self._lock:
            self.config_path.write_text(f"{payload}\n", encoding="utf-8")

    def update_server(self, update: ServerConfigUpdate) -> AppConfig:
        config = self.load()
        config.server = ServerConfig(url=update.url.strip(), api_key=update.api_key.strip())
        return self.save(config)

    def list_jobs(self) -> list[JobConfig]:
        return self.load().jobs

    def get_job(self, job_id: str) -> JobConfig | None:
        for job in self.load().jobs:
            if job.id == job_id:
                return job
        return None

    def create_job(self, request: JobUpsertRequest) -> JobConfig:
        config = self.load()
        job = JobConfig(id=str(uuid4()), **request.model_dump())
        config.jobs.append(job)
        self.save(config)
        return job

    def update_job(self, job_id: str, request: JobUpsertRequest) -> JobConfig | None:
        config = self.load()
        for index, existing in enumerate(config.jobs):
            if existing.id == job_id:
                updated = JobConfig(id=job_id, **request.model_dump())
                config.jobs[index] = updated
                self.save(config)
                return updated
        return None

    def delete_job(self, job_id: str) -> bool:
        config = self.load()
        next_jobs = [job for job in config.jobs if job.id != job_id]
        if len(next_jobs) == len(config.jobs):
            return False
        config.jobs = next_jobs
        self.save(config)
        return True

    def set_job_enabled(self, job_id: str, enabled: bool) -> JobConfig | None:
        config = self.load()
        for index, job in enumerate(config.jobs):
            if job.id == job_id:
                config.jobs[index] = job.model_copy(update={"enabled": enabled})
                self.save(config)
                return config.jobs[index]
        return None
