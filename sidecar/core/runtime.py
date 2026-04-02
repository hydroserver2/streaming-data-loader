from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from sidecar.core.config import ConfigStore
from sidecar.core.hydroserver import HydroServerService
from sidecar.core.scheduler import SchedulerService
from sidecar.core.state import StateStore


APP_VERSION = "0.1.0"


@dataclass
class RuntimeSettings:
    host: str
    port: int
    config_dir: Path
    version: str = APP_VERSION


@dataclass
class AppRuntime:
    settings: RuntimeSettings
    config_store: ConfigStore
    state_store: StateStore
    hydroserver: HydroServerService
    scheduler: SchedulerService
    running_jobs: set[str] = field(default_factory=set)
