from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import AliasChoices, BaseModel, ConfigDict, Field, model_validator


JobStatus = Literal["healthy", "warning", "error", "disabled", "pending", "running"]
ConnectionState = Literal["not_configured", "configured", "connected", "error"]
LogLevel = Literal["info", "warning", "error"]
AuthType = Literal["apikey", "userpass"]
CsvIdentifierType = Literal["name", "index"]
CsvDelimiterType = Literal[",", ";", "\t", "|", " "]
TimestampFormatType = Literal["ISO8601", "naive", "custom"]
TimezoneModeType = Literal["embeddedOffset", "utc", "fixedOffset", "daylightSavings"]


class ServerConfig(BaseModel):
    auth_type: AuthType = "apikey"
    url: str = ""
    api_key: str = ""
    username: str = ""
    password: str = ""
    workspace_id: str = ""


class TimestampConfig(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    key: str = Field(default="timestamp", min_length=1)
    format: TimestampFormatType = "ISO8601"
    custom_format: str | None = Field(
        default=None,
        validation_alias=AliasChoices("customFormat", "custom_format"),
        serialization_alias="customFormat",
    )
    timezone_mode: TimezoneModeType = Field(
        default="embeddedOffset",
        validation_alias=AliasChoices("timezoneMode", "timezone_mode"),
        serialization_alias="timezoneMode",
    )
    timezone: str | None = None

    @model_validator(mode="after")
    def normalize_timestamp(self) -> "TimestampConfig":
        if self.format == "custom":
            if not (self.custom_format or "").strip():
                raise ValueError("Custom timestamp formats require a customFormat value.")
        else:
            self.custom_format = None

        if self.format == "ISO8601":
            self.timezone_mode = "embeddedOffset"
            self.timezone = None
            return self

        if self.timezone_mode == "embeddedOffset":
            self.timezone_mode = "utc"

        if self.timezone_mode == "utc":
            self.timezone = None
        elif not (self.timezone or "").strip():
            raise ValueError(
                "Timezone is required when using fixedOffset or daylightSavings timestamp modes."
            )

        return self


class FileConfig(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    header_row: int | None = Field(
        default=1,
        ge=1,
        validation_alias=AliasChoices("headerRow", "header_row"),
        serialization_alias="headerRow",
    )
    data_start_row: int = Field(
        default=2,
        ge=1,
        validation_alias=AliasChoices("dataStartRow", "data_start_row"),
        serialization_alias="dataStartRow",
    )
    delimiter: CsvDelimiterType = ","
    identifier_type: CsvIdentifierType = Field(
        default="name",
        validation_alias=AliasChoices("identifierType", "identifier_type"),
        serialization_alias="identifierType",
    )
    timestamp: TimestampConfig = Field(default_factory=TimestampConfig)

    @model_validator(mode="before")
    @classmethod
    def migrate_legacy_settings(cls, value):
        if not isinstance(value, dict):
            return value

        if "timestamp" in value or "identifierType" in value or "identifier_type" in value:
            return value

        legacy_key = value.get("timestamp_column") or value.get("timestampColumn")
        legacy_format = value.get("timestamp_format") or value.get("timestampFormat")
        legacy_timezone = value.get("timezone")

        timestamp: dict[str, str] = {
            "key": legacy_key or "timestamp",
        }

        if legacy_format:
            timestamp["format"] = "custom"
            timestamp["customFormat"] = legacy_format
        else:
            timestamp["format"] = "ISO8601"

        if legacy_timezone:
            if "/" in legacy_timezone:
                timestamp["timezoneMode"] = "daylightSavings"
                timestamp["timezone"] = legacy_timezone
            elif legacy_timezone.upper() == "UTC":
                timestamp["timezoneMode"] = "utc"
            else:
                timestamp["timezoneMode"] = "fixedOffset"
                timestamp["timezone"] = legacy_timezone

            if timestamp["format"] == "ISO8601":
                timestamp["format"] = "naive"
        else:
            timestamp["timezoneMode"] = "embeddedOffset"

        return {
            "headerRow": value.get("headerRow", value.get("header_row", 1)),
            "dataStartRow": value.get("dataStartRow", value.get("data_start_row", 2)),
            "delimiter": value.get("delimiter", ","),
            "identifierType": value.get("identifierType", value.get("identifier_type", "name")),
            "timestamp": timestamp,
        }

    @model_validator(mode="after")
    def normalize_csv_settings(self) -> "FileConfig":
        if self.identifier_type == "index":
            self.header_row = None
            try:
                timestamp_index = int(self.timestamp.key)
            except (TypeError, ValueError) as exc:
                raise ValueError(
                    "timestamp.key must be a positive integer when using index-based CSV identifiers."
                ) from exc
            if timestamp_index <= 0:
                raise ValueError(
                    "timestamp.key must be a positive integer when using index-based CSV identifiers."
                )
        elif self.header_row is None:
            raise ValueError("headerRow is required when using name-based column identifiers.")

        if self.header_row is not None and self.data_start_row <= self.header_row:
            raise ValueError("dataStartRow must be greater than headerRow when a header row is configured.")

        return self


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
    workspace_id: str = ""

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
    workspace_id: str = ""

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
    workspace_id: str | None = None
    workspace_name: str | None = None
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
    thing_id: str = ""
    thing_name: str = ""
    observed_property_name: str = ""
    processing_level_definition: str = ""
    unit_name: str = ""
    unit_symbol: str = ""
    sampled_medium: str = ""
    sensor_name: str = ""
    result_type: str = ""


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
