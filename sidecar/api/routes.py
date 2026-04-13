from __future__ import annotations

import logging
import time
from contextlib import asynccontextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path

from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException, Query, Request
from fastapi.middleware.cors import CORSMiddleware

from sidecar.api.models import (
    ActionResponse,
    AppConfig,
    ConnectionStatus,
    ConnectionTestRequest,
    ConnectionTestResponse,
    CsvPreviewResponse,
    DatastreamSummary,
    HealthResponse,
    JobConfig,
    JobCursor,
    JobDetail,
    JobLogEntry,
    JobStatusSummary,
    JobUpsertRequest,
    ServerConfig,
    ServerConfigUpdate,
    ServerUrlValidationResponse,
)
from sidecar.core.loader import preview_csv
from sidecar.core.runtime import AppRuntime


log = logging.getLogger(__name__)


def create_app(runtime: AppRuntime) -> FastAPI:
    @asynccontextmanager
    async def lifespan(app: FastAPI):
        runtime.config_store.ensure()
        runtime.state_store.ensure()
        runtime.scheduler.start()
        runtime.scheduler.sync_jobs(
            [job.id for job in runtime.config_store.list_jobs()]
        )
        app.state.runtime = runtime
        try:
            yield
        finally:
            runtime.scheduler.shutdown()

    app = FastAPI(
        title="HydroServer Streaming Data Loader",
        version=runtime.settings.version,
        lifespan=lifespan,
    )
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["http://localhost:1420", "tauri://localhost"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    @app.get("/health", response_model=HealthResponse, tags=["health"])
    def health(runtime: AppRuntime = Depends(get_runtime)) -> HealthResponse:
        config = runtime.config_store.load()
        connection = _connection_status(config.server)
        return HealthResponse(
            version=runtime.settings.version,
            config_dir=str(runtime.settings.config_dir),
            server_configured=_server_is_configured(config.server),
            connection=connection,
        )

    @app.get("/config", response_model=AppConfig, tags=["config"])
    def get_config(runtime: AppRuntime = Depends(get_runtime)) -> AppConfig:
        return runtime.config_store.load()

    @app.put("/config/server", response_model=AppConfig, tags=["config"])
    def update_server_config(
        payload: ServerConfigUpdate,
        runtime: AppRuntime = Depends(get_runtime),
    ) -> AppConfig:
        server = ServerConfig(**payload.model_dump())
        connection = runtime.hydroserver.test_connection(server)
        if not connection.ok:
            status_code = (
                502
                if connection.message.startswith("Couldn't reach HydroServer")
                or connection.message.startswith("HydroServer returned an error")
                or connection.message.startswith("Couldn't complete")
                else 400
            )
            raise HTTPException(status_code=status_code, detail=connection.message)

        return runtime.config_store.set_server(
            server.model_copy(update={"workspace_id": connection.workspace_id or ""}),
            workspace_name=connection.workspace_name or "",
        )

    @app.delete("/config/server", response_model=AppConfig, tags=["config"])
    def clear_server_config(runtime: AppRuntime = Depends(get_runtime)) -> AppConfig:
        return runtime.config_store.clear_server()

    @app.post(
        "/connection/test", response_model=ConnectionTestResponse, tags=["connection"]
    )
    def test_connection(
        payload: ConnectionTestRequest,
        runtime: AppRuntime = Depends(get_runtime),
    ) -> ConnectionTestResponse:
        result = runtime.hydroserver.test_connection(
            ServerConfig(**payload.model_dump())
        )
        return ConnectionTestResponse(
            ok=result.ok,
            state=result.state,  # type: ignore[arg-type]
            message=result.message,
            instance_name=result.instance_name,
            workspace_id=result.workspace_id,
            workspace_name=result.workspace_name,
            workspace_count=result.workspace_count,
            datastream_count=result.datastream_count,
            permissions_ok=result.permissions_ok,
        )

    @app.get(
        "/connection/validate-url",
        response_model=ServerUrlValidationResponse,
        tags=["connection"],
    )
    def validate_server_url(
        url: str = Query(..., description="HydroServer base URL"),
        runtime: AppRuntime = Depends(get_runtime),
    ) -> ServerUrlValidationResponse:
        result = runtime.hydroserver.validate_url(url)
        return ServerUrlValidationResponse(
            ok=result.ok,
            message=result.message,
            instance_name=result.instance_name,
        )

    @app.get("/jobs", response_model=list[JobStatusSummary], tags=["jobs"])
    def list_jobs(runtime: AppRuntime = Depends(get_runtime)) -> list[JobStatusSummary]:
        return [
            _build_job_summary(runtime, job) for job in runtime.config_store.list_jobs()
        ]

    @app.post("/jobs", response_model=JobDetail, tags=["jobs"])
    def create_job(
        payload: JobUpsertRequest,
        runtime: AppRuntime = Depends(get_runtime),
    ) -> JobDetail:
        job = runtime.config_store.create_job(payload)
        runtime.state_store.append_log(job.id, "Job created")
        runtime.scheduler.sync_jobs(
            [item.id for item in runtime.config_store.list_jobs()]
        )
        return _build_job_detail(runtime, job)

    @app.get("/jobs/{job_id}", response_model=JobDetail, tags=["jobs"])
    def get_job(job_id: str, runtime: AppRuntime = Depends(get_runtime)) -> JobDetail:
        job = runtime.config_store.get_job(job_id)
        if job is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        return _build_job_detail(runtime, job)

    @app.put("/jobs/{job_id}", response_model=JobDetail, tags=["jobs"])
    def update_job(
        job_id: str,
        payload: JobUpsertRequest,
        runtime: AppRuntime = Depends(get_runtime),
    ) -> JobDetail:
        job = runtime.config_store.update_job(job_id, payload)
        if job is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        runtime.state_store.append_log(job.id, "Job updated")
        runtime.scheduler.sync_jobs(
            [item.id for item in runtime.config_store.list_jobs()]
        )
        return _build_job_detail(runtime, job)

    @app.delete("/jobs/{job_id}", response_model=ActionResponse, tags=["jobs"])
    def delete_job(
        job_id: str, runtime: AppRuntime = Depends(get_runtime)
    ) -> ActionResponse:
        deleted = runtime.config_store.delete_job(job_id)
        if not deleted:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        runtime.state_store.delete_job(job_id)
        runtime.scheduler.sync_jobs(
            [item.id for item in runtime.config_store.list_jobs()]
        )
        return ActionResponse(message="Job deleted.")

    @app.post("/jobs/{job_id}/run", response_model=ActionResponse, tags=["jobs"])
    def run_job_now(
        job_id: str,
        background_tasks: BackgroundTasks,
        runtime: AppRuntime = Depends(get_runtime),
    ) -> ActionResponse:
        job = runtime.config_store.get_job(job_id)
        if job is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        if job_id in runtime.running_jobs:
            return ActionResponse(message="Job is already running.")
        runtime.running_jobs.add(job_id)
        runtime.state_store.append_log(job_id, "Manual run started")
        background_tasks.add_task(_simulate_job_run, runtime, job)
        return ActionResponse(message="Job started.")

    @app.post("/jobs/{job_id}/enable", response_model=ActionResponse, tags=["jobs"])
    def enable_job(
        job_id: str, runtime: AppRuntime = Depends(get_runtime)
    ) -> ActionResponse:
        job = runtime.config_store.set_job_enabled(job_id, True)
        if job is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        runtime.state_store.append_log(job_id, "Job enabled")
        return ActionResponse(message="Job enabled.")

    @app.post("/jobs/{job_id}/disable", response_model=ActionResponse, tags=["jobs"])
    def disable_job(
        job_id: str, runtime: AppRuntime = Depends(get_runtime)
    ) -> ActionResponse:
        job = runtime.config_store.set_job_enabled(job_id, False)
        if job is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        runtime.state_store.append_log(job_id, "Job disabled", level="warning")
        return ActionResponse(message="Job disabled.")

    @app.get("/jobs/{job_id}/logs", response_model=list[JobLogEntry], tags=["jobs"])
    def get_job_logs(
        job_id: str, runtime: AppRuntime = Depends(get_runtime)
    ) -> list[JobLogEntry]:
        if runtime.config_store.get_job(job_id) is None:
            raise HTTPException(status_code=404, detail="That job could not be found.")
        return runtime.state_store.logs_for(job_id)

    @app.get(
        "/datastreams", response_model=list[DatastreamSummary], tags=["hydroserver"]
    )
    def get_datastreams(
        runtime: AppRuntime = Depends(get_runtime),
    ) -> list[DatastreamSummary]:
        config = runtime.config_store.load()
        try:
            return runtime.hydroserver.list_datastreams(config.server)
        except Exception as exc:
            log.warning("Failed to load datastreams: %s", exc)
            raise HTTPException(
                status_code=502,
                detail="Couldn't load datastreams from HydroServer right now.",
            ) from exc

    @app.get("/csv/preview", response_model=CsvPreviewResponse, tags=["csv"])
    def get_csv_preview(
        path: str = Query(..., description="Absolute path to the CSV file"),
        rows: int = Query(default=100, ge=1, le=500),
    ) -> CsvPreviewResponse:
        try:
            return preview_csv(path=path, rows=rows)
        except FileNotFoundError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc
        except UnicodeDecodeError as exc:
            raise HTTPException(
                status_code=400,
                detail="Couldn't read the file encoding. Try exporting as UTF-8.",
            ) from exc

    return app


def get_runtime(request: Request) -> AppRuntime:
    return request.app.state.runtime  # type: ignore[no-any-return]


def _build_job_summary(runtime: AppRuntime, job: JobConfig) -> JobStatusSummary:
    cursor = runtime.state_store.cursor_for(job.id)
    status, status_message = _derive_job_status(
        job, cursor, job.id in runtime.running_jobs
    )
    return JobStatusSummary(
        **job.model_dump(),
        status=status,
        status_message=status_message,
        last_pushed_timestamp=cursor.last_pushed_timestamp,
        last_run_at=cursor.last_run_at,
        last_error=cursor.last_error,
    )


def _build_job_detail(runtime: AppRuntime, job: JobConfig) -> JobDetail:
    summary = _build_job_summary(runtime, job)
    return JobDetail(
        **summary.model_dump(), recent_logs=runtime.state_store.logs_for(job.id)
    )


def _derive_job_status(
    job: JobConfig, cursor: JobCursor, is_running: bool
) -> tuple[str, str]:
    if is_running:
        return "running", "Running now"
    if not job.enabled:
        return "disabled", "Paused"
    if cursor.last_error:
        return "error", cursor.last_error
    if cursor.last_pushed_timestamp is None:
        return "pending", "Ready for the first run"
    if cursor.last_run_at is not None:
        stale_after = timedelta(minutes=max(job.schedule_minutes * 2, 1))
        if datetime.now(timezone.utc) - cursor.last_run_at > stale_after:
            return "warning", "This job looks stale"
    return "healthy", "Last push succeeded"


def _connection_status(server: ServerConfig) -> ConnectionStatus:
    if not _server_is_configured(server):
        return ConnectionStatus(
            state="not_configured", message="HydroServer not configured"
        )
    return ConnectionStatus(state="configured", message="HydroServer configured")


def _server_is_configured(server: ServerConfig) -> bool:
    if not server.url.strip():
        return False
    if server.auth_type == "userpass":
        return bool(server.username.strip() and server.password.strip())
    return bool(server.api_key.strip())


def _simulate_job_run(runtime: AppRuntime, job: JobConfig) -> None:
    try:
        time.sleep(1.2)
        cursor = runtime.state_store.cursor_for(job.id)
        now = datetime.now(timezone.utc)
        updated_cursor = cursor.model_copy(
            update={
                "last_run_at": now,
                "last_pushed_timestamp": now,
                "last_pushed_row_index": (cursor.last_pushed_row_index or 0) + 25,
                "last_error": None,
            }
        )
        runtime.state_store.update_cursor(job.id, updated_cursor)
        runtime.state_store.append_log(
            job.id,
            "Stub run completed. Real file loading and HydroServer push are the next implementation phase.",
        )
    finally:
        runtime.running_jobs.discard(job.id)
