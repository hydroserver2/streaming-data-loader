from __future__ import annotations

from datetime import datetime, timezone

from sidecar.api.models import JobCursor, JobLogEntry, LogLevel
from sidecar.core.config import ConfigStore


class StateStore:
    def __init__(self, config_store: ConfigStore) -> None:
        self.config_store = config_store

    def ensure(self) -> None:
        self.config_store.ensure()

    def load(self) -> None:
        self.ensure()
        return None

    def save(self, state) -> None:
        return state

    def cursor_for(self, job_id: str) -> JobCursor:
        return self.config_store.cursor_for(job_id)

    def logs_for(self, job_id: str, limit: int = 50) -> list[JobLogEntry]:
        return self.config_store.logs_for(job_id, limit=limit)

    def update_cursor(self, job_id: str, cursor: JobCursor) -> JobCursor:
        return self.config_store.update_cursor(job_id, cursor)

    def append_log(self, job_id: str, message: str, level: LogLevel = "info") -> JobLogEntry:
        entry = JobLogEntry(timestamp=datetime.now(timezone.utc), level=level, message=message)
        self.config_store.append_log(job_id, entry)
        return entry

    def delete_job(self, job_id: str) -> None:
        self.config_store.delete_job_runtime(job_id)
