from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field, model_validator


JobStatus = Literal["healthy", "warning", "error", "disabled", "pending", "running"]
ConnectionState = Literal["not_configured", "configured", "connected", "error"]
LogLevel = Literal["info", "warning", "error"]
AuthType = Literal["apikey", "userpass"]


class ServerConfig(BaseModel):
    auth_type: AuthType = "apikey"
    url: str = ""
    api_key: str = ""
    username: str = ""
    password: str = ""


class FileConfig(BaseModel):
    header_row: int = Field(default=3, ge=0)
    data_start_row: int = Field(default=4, ge=1)
    delimiter: str = Field(default=",", min_length=1, max_length=2)
    timestamp_column: str = Field(default="Timestamp", min_length=1)
    timestamp_format: str = Field(default="%Y-%m-%d %H:%M:%S", min_length=1)
    timezone: str = Field(default="America/Denver", min_length=1)


class ColumnMapping(BaseModel):
    csv_column: str = Field(min_length=1)
    datastream_id: str = Field(min_length=1)
    datastream_name: str = Field(min_length=1)


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
    auth_type: AuthType = "apikey"
    url: str
    api_key: str
    username: str = ""
    password: str = ""

    @model_validator(mode="after")
    def validate_auth_fields(self) -> "ServerConfigUpdate":
        if not self.url.strip():
            raise ValueError("Host URL is required.")
        if self.auth_type == "apikey" and not self.api_key.strip():
            raise ValueError("API key is required.")
        if self.auth_type == "userpass" and (
            not self.username.strip() or not self.password.strip()
        ):
            raise ValueError("Username and password are required.")
        return self


class ConnectionTestRequest(BaseModel):
    auth_type: AuthType = "apikey"
    url: str
    api_key: str
    username: str = ""
    password: str = ""

    @model_validator(mode="after")
    def validate_auth_fields(self) -> "ConnectionTestRequest":
        if not self.url.strip():
            raise ValueError("Host URL is required.")
        if self.auth_type == "apikey" and not self.api_key.strip():
            raise ValueError("API key is required.")
        if self.auth_type == "userpass" and (
            not self.username.strip() or not self.password.strip()
        ):
            raise ValueError("Username and password are required.")
        return self


class ConnectionTestResponse(BaseModel):
    ok: bool
    state: ConnectionState
    message: str
    instance_name: str | None = None
    workspace_count: int = 0
    datastream_count: int = 0
    permissions_ok: bool = False


class ServerUrlValidationResponse(BaseModel):
    ok: bool
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
    name: str = Field(min_length=1)
    enabled: bool = True
    file_path: str = Field(min_length=1)
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
