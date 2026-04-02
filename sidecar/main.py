from __future__ import annotations

import argparse
import logging
import os
from pathlib import Path

import platformdirs
import uvicorn

from sidecar.api.routes import create_app
from sidecar.core.config import ConfigStore
from sidecar.core.hydroserver import HydroServerService
from sidecar.core.runtime import AppRuntime, RuntimeSettings
from sidecar.core.scheduler import SchedulerService
from sidecar.core.state import StateStore


def default_config_dir() -> Path:
    return Path(platformdirs.user_data_dir("com.hydroserver.sdl"))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default=os.environ.get("SDL_SIDECAR_HOST", "127.0.0.1"))
    parser.add_argument("--port", type=int, default=int(os.environ.get("SDL_SIDECAR_PORT", "8765")))
    parser.add_argument(
        "--reload",
        action="store_true",
        default=os.environ.get("SDL_SIDECAR_RELOAD", "").lower() in {"1", "true", "yes"},
    )
    parser.add_argument(
        "--config-dir",
        default=os.environ.get("SDL_CONFIG_DIR", str(default_config_dir())),
    )
    return parser.parse_args()


def build_runtime() -> AppRuntime:
    args = parse_args()
    config_dir = Path(args.config_dir).expanduser()
    if not config_dir.is_absolute():
        config_dir = (Path.cwd() / config_dir).resolve()

    settings = RuntimeSettings(host=args.host, port=args.port, config_dir=config_dir)
    return AppRuntime(
        settings=settings,
        config_store=ConfigStore(config_dir),
        state_store=StateStore(config_dir),
        hydroserver=HydroServerService(),
        scheduler=SchedulerService(),
    )


def run() -> None:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    args = parse_args()

    if args.reload:
        uvicorn.run(
            "sidecar.main:create_dev_app",
            host=args.host,
            port=args.port,
            reload=True,
            factory=True,
        )
        return

    runtime = build_runtime()
    app = create_app(runtime)
    uvicorn.run(app, host=runtime.settings.host, port=runtime.settings.port, reload=False)


def create_dev_app():
    runtime = build_runtime()
    return create_app(runtime)


if __name__ == "__main__":
    run()
