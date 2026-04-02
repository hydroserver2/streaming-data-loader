from __future__ import annotations

import logging


log = logging.getLogger(__name__)


class SchedulerService:
    def start(self) -> None:
        log.info("Scheduler placeholder started")

    def sync_jobs(self, job_ids: list[str]) -> None:
        log.info("Scheduler placeholder synced %s jobs", len(job_ids))

    def shutdown(self) -> None:
        log.info("Scheduler placeholder stopped")
