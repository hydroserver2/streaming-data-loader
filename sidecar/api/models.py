from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field


JobStatus = Literal["healthy", "warning", "error", "disabled", "pending", "running"]
ConnectionState = Literal["not_configured", "configured", "connected", "error"]
LogLevel = Literal["info", "warning", "error"]


class ServerConfig(BaseModel):
    url: str = ""
    api_key: str = ""


class FileConfig(BaseModel):
    header_row: int = 3
    data_start_row: int = 4
    delimiter: str = ","
    timestamp_column: str = "Timestamp"
    timestamp_format: str = "%Y-%m-%d %H:%M:%S"
    timezone: str = "America/Denver"


class ColumnMapping(BaseModel):
    csv_column: str
    datastream_id: str
    datastream_name: str


class JobConfig(BaseModel):
    id: str
    name: str
    enabled: bool = True
    file_path: str
    schedule_minutes: int = 15
    file_config: FileConfig = Field(default_factory=FileConfig)
    column_mappings: list[ColumnMapping] = Field(default_factory=list)


class AppConfig(BaseModel):
    version: int = 1
    server: ServerConfig = Field(default_factory=ServerConfig)
    jobs: list[JobConfig] = Field(default_factory=list)


class JobCursor(BaseModel):
    last_pushed_timestamp: datetime | None = None
    last_pushed_row_index: int | None = None
    last_run_at: datetime | None = None
    last_error: str | None = None


class JobLogEntry(BaseModel):
    timestamp: datetime
    level: LogLevel
    message: str


class AppStateFile(BaseModel):
    cursors: dict[str, JobCursor] = Field(default_factory=dict)
    logs: dict[str, list[JobLogEntry]] = Field(default_factory=dict)


class ConnectionStatus(BaseModel):
    state: ConnectionState
    message: str


class HealthResponse(BaseModel):
    status: Literal["ok"] = "ok"
    version: str
    config_dir: str
    server_configured: bool
    connection: ConnectionStatus


class ServerConfigUpdate(BaseModel):
    url: str
    api_key: str


class ConnectionTestRequest(BaseModel):
    url: str
    api_key: str


class ConnectionTestResponse(BaseModel):
    ok: bool
    state: ConnectionState
    message: str
    instance_name: str | None = None


class ActionResponse(BaseModel):
    ok: bool = True
    message: str


class DatastreamSummary(BaseModel):
    id: str
    name: str


class JobStatusSummary(BaseModel):
    id: str
    name: str
    enabled: bool
    file_path: str
    schedule_minutes: int
    file_config: FileConfig
    column_mappings: list[ColumnMapping]
    status: JobStatus
    status_message: str
    last_pushed_timestamp: datetime | None = None
    last_run_at: datetime | None = None
    last_error: str | None = None


class JobDetail(JobStatusSummary):
    recent_logs: list[JobLogEntry] = Field(default_factory=list)


class JobUpsertRequest(BaseModel):
    name: str
    enabled: bool = True
    file_path: str
    schedule_minutes: int = Field(default=15, ge=1)
    file_config: FileConfig = Field(default_factory=FileConfig)
    column_mappings: list[ColumnMapping] = Field(default_factory=list)


class CsvPreviewResponse(BaseModel):
    raw_lines: list[str]
    parsed_rows: list[list[str]]
    detected_header_row: int | None
    detected_data_start_row: int | None
    detected_delimiter: str
    total_lines: int
    encoding: str
