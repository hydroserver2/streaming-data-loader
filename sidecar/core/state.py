from __future__ import annotations

import json
import threading
from datetime import datetime, timezone
from pathlib import Path

from sidecar.api.models import AppStateFile, JobCursor, JobLogEntry, LogLevel


class StateStore:
    def __init__(self, config_dir: Path) -> None:
        self.config_dir = config_dir
        self.state_path = config_dir / "state.json"
        self._lock = threading.Lock()

    def ensure(self) -> None:
        self.config_dir.mkdir(parents=True, exist_ok=True)
        if not self.state_path.exists():
            self._write(AppStateFile())

    def load(self) -> AppStateFile:
        self.ensure()
        with self._lock:
            return AppStateFile.model_validate_json(self.state_path.read_text(encoding="utf-8"))

    def save(self, state: AppStateFile) -> AppStateFile:
        self.config_dir.mkdir(parents=True, exist_ok=True)
        self._write(state)
        return state

    def _write(self, state: AppStateFile) -> None:
        payload = json.dumps(state.model_dump(mode="json"), indent=2)
        with self._lock:
            self.state_path.write_text(f"{payload}\n", encoding="utf-8")

    def cursor_for(self, job_id: str) -> JobCursor:
        return self.load().cursors.get(job_id, JobCursor())

    def logs_for(self, job_id: str, limit: int = 50) -> list[JobLogEntry]:
        logs = self.load().logs.get(job_id, [])
        return logs[-limit:]

    def update_cursor(self, job_id: str, cursor: JobCursor) -> JobCursor:
        state = self.load()
        state.cursors[job_id] = cursor
        self.save(state)
        return cursor

    def append_log(self, job_id: str, message: str, level: LogLevel = "info") -> JobLogEntry:
        state = self.load()
        entry = JobLogEntry(timestamp=datetime.now(timezone.utc), level=level, message=message)
        state.logs.setdefault(job_id, []).append(entry)
        state.logs[job_id] = state.logs[job_id][-50:]
        self.save(state)
        return entry

    def delete_job(self, job_id: str) -> None:
        state = self.load()
        state.cursors.pop(job_id, None)
        state.logs.pop(job_id, None)
        self.save(state)
