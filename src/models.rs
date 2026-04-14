use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    #[default]
    Apikey,
    Userpass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    #[default]
    NotConfigured,
    Configured,
    Connected,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Healthy,
    Warning,
    Error,
    Disabled,
    #[default]
    Pending,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimestampFormatType {
    #[default]
    #[serde(rename = "ISO8601")]
    Iso8601,
    #[serde(rename = "naive")]
    Naive,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IdentifierType {
    #[default]
    Name,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimezoneModeType {
    #[default]
    #[serde(rename = "embeddedOffset")]
    EmbeddedOffset,
    #[serde(rename = "utc")]
    Utc,
    #[serde(rename = "fixedOffset")]
    FixedOffset,
    #[serde(rename = "daylightSavings")]
    DaylightSavings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub auth_type: AuthType,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub workspace_id: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            auth_type: AuthType::Apikey,
            url: String::new(),
            api_key: String::new(),
            username: String::new(),
            password: String::new(),
            workspace_id: String::new(),
        }
    }
}

impl ServerConfig {
    pub fn normalized(mut self) -> Self {
        self.url = normalize_url(&self.url);
        self.api_key = self.api_key.trim().to_string();
        self.username = self.username.trim().to_string();
        self.password = self.password.trim().to_string();
        self.workspace_id = self.workspace_id.trim().to_string();

        match self.auth_type {
            AuthType::Apikey => {
                self.username.clear();
                self.password.clear();
            }
            AuthType::Userpass => {
                self.api_key.clear();
            }
        }

        self
    }

    pub fn validated_for_connection(self) -> Result<Self, String> {
        let server = self.normalized();

        if server.url.is_empty() {
            return Err("Host URL is required.".to_string());
        }

        match server.auth_type {
            AuthType::Apikey if server.api_key.is_empty() => {
                Err("API key is required.".to_string())
            }
            AuthType::Userpass if server.username.is_empty() || server.password.is_empty() => {
                Err("Username and password are required.".to_string())
            }
            _ => Ok(server),
        }
    }

    pub fn is_configured(&self) -> bool {
        if self.url.trim().is_empty() {
            return false;
        }

        match self.auth_type {
            AuthType::Apikey => !self.api_key.trim().is_empty(),
            AuthType::Userpass => {
                !self.username.trim().is_empty() && !self.password.trim().is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampConfig {
    #[serde(default = "default_timestamp_key")]
    pub key: String,
    #[serde(default)]
    pub format: TimestampFormatType,
    #[serde(default, rename = "customFormat", alias = "custom_format")]
    pub custom_format: Option<String>,
    #[serde(default, rename = "timezoneMode", alias = "timezone_mode")]
    pub timezone_mode: TimezoneModeType,
    #[serde(default)]
    pub timezone: Option<String>,
}

impl Default for TimestampConfig {
    fn default() -> Self {
        Self {
            key: default_timestamp_key(),
            format: TimestampFormatType::Iso8601,
            custom_format: None,
            timezone_mode: TimezoneModeType::EmbeddedOffset,
            timezone: None,
        }
    }
}

impl TimestampConfig {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.key = self.key.trim().to_string();
        self.custom_format = self
            .custom_format
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.timezone = self
            .timezone
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if self.key.is_empty() {
            self.key = default_timestamp_key();
        }

        match self.format {
            TimestampFormatType::Custom => {
                if self.custom_format.is_none() {
                    return Err(
                        "Custom timestamp formats require a customFormat value.".to_string()
                    );
                }
            }
            TimestampFormatType::Iso8601 | TimestampFormatType::Naive => {
                self.custom_format = None;
            }
        }

        if self.format == TimestampFormatType::Iso8601 {
            self.timezone_mode = TimezoneModeType::EmbeddedOffset;
            self.timezone = None;
            return Ok(self);
        }

        if self.timezone_mode == TimezoneModeType::EmbeddedOffset {
            self.timezone_mode = TimezoneModeType::Utc;
        }

        match self.timezone_mode {
            TimezoneModeType::Utc => {
                self.timezone = None;
            }
            TimezoneModeType::FixedOffset | TimezoneModeType::DaylightSavings => {
                if self.timezone.is_none() {
                    return Err(
                        "Timezone is required when using fixedOffset or daylightSavings timestamp modes."
                            .to_string(),
                    );
                }
            }
            TimezoneModeType::EmbeddedOffset => {}
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileConfig {
    #[serde(
        default = "default_optional_header_row",
        rename = "headerRow",
        alias = "header_row"
    )]
    pub header_row: Option<u32>,
    #[serde(
        default = "default_data_start_row",
        rename = "dataStartRow",
        alias = "data_start_row"
    )]
    pub data_start_row: u32,
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    #[serde(default, rename = "identifierType", alias = "identifier_type")]
    pub identifier_type: IdentifierType,
    #[serde(default)]
    pub timestamp: TimestampConfig,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            header_row: Some(default_header_row()),
            data_start_row: default_data_start_row(),
            delimiter: default_delimiter(),
            identifier_type: IdentifierType::default(),
            timestamp: TimestampConfig::default(),
        }
    }
}

impl FileConfig {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.delimiter = if self.delimiter.is_empty() {
            default_delimiter()
        } else {
            self.delimiter
        };
        self.timestamp = self.timestamp.normalized()?;

        if !matches!(self.delimiter.as_str(), "," | ";" | "\t" | "|" | " ") {
            return Err("Delimiter must be one of ',', ';', tab, '|', or space.".to_string());
        }

        match self.identifier_type {
            IdentifierType::Index => {
                self.header_row = None;
                let timestamp_index = self.timestamp.key.parse::<u32>().map_err(|_| {
                    "timestamp.key must be a positive integer when using index-based CSV identifiers."
                        .to_string()
                })?;

                if timestamp_index == 0 {
                    return Err(
                        "timestamp.key must be a positive integer when using index-based CSV identifiers."
                            .to_string(),
                    );
                }
            }
            IdentifierType::Name => {
                if self.header_row.is_none() {
                    return Err(
                        "headerRow is required when using name-based column identifiers."
                            .to_string(),
                    );
                }
            }
        }

        if let Some(header_row) = self.header_row {
            if self.data_start_row <= header_row {
                return Err(
                    "dataStartRow must be greater than headerRow when a header row is configured."
                        .to_string(),
                );
            }
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMapping {
    pub csv_column: String,
    pub datastream_id: String,
    pub datastream_name: String,
}

impl ColumnMapping {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.csv_column = self.csv_column.trim().to_string();
        self.datastream_id = self.datastream_id.trim().to_string();
        self.datastream_name = self.datastream_name.trim().to_string();

        if self.csv_column.is_empty()
            || self.datastream_id.is_empty()
            || self.datastream_name.is_empty()
        {
            return Err(
                "Column mappings require csv_column, datastream_id, and datastream_name."
                    .to_string(),
            );
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobUpsertRequest {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub file_path: String,
    #[serde(default = "default_schedule_minutes")]
    pub schedule_minutes: u32,
    pub file_config: FileConfig,
    #[serde(default)]
    pub column_mappings: Vec<ColumnMapping>,
}

impl JobUpsertRequest {
    pub fn normalized(mut self) -> Result<Self, String> {
        self.name = self.name.trim().to_string();
        self.file_path = self.file_path.trim().to_string();
        self.file_config = self.file_config.normalized()?;
        self.column_mappings = self
            .column_mappings
            .into_iter()
            .map(ColumnMapping::normalized)
            .collect::<Result<Vec<_>, _>>()?;

        if self.name.is_empty() {
            return Err("Job name is required.".to_string());
        }
        if self.file_path.is_empty() {
            return Err("File path is required.".to_string());
        }
        if self.schedule_minutes == 0 {
            return Err("schedule_minutes must be greater than 0.".to_string());
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub file_path: String,
    #[serde(default = "default_schedule_minutes")]
    pub schedule_minutes: u32,
    pub file_config: FileConfig,
    #[serde(default)]
    pub column_mappings: Vec<ColumnMapping>,
}

impl JobConfig {
    pub fn from_request(id: String, request: JobUpsertRequest) -> Result<Self, String> {
        let request = request.normalized()?;
        Ok(Self {
            id,
            name: request.name,
            enabled: request.enabled,
            file_path: request.file_path,
            schedule_minutes: request.schedule_minutes,
            file_config: request.file_config,
            column_mappings: request.column_mappings,
        })
    }

    pub fn normalized(mut self) -> Result<Self, String> {
        self.id = self.id.trim().to_string();
        if self.id.is_empty() {
            return Err("Job id is required.".to_string());
        }

        let request = JobUpsertRequest {
            name: self.name,
            enabled: self.enabled,
            file_path: self.file_path,
            schedule_minutes: self.schedule_minutes,
            file_config: self.file_config,
            column_mappings: self.column_mappings,
        }
        .normalized()?;

        self.name = request.name;
        self.enabled = request.enabled;
        self.file_path = request.file_path;
        self.schedule_minutes = request.schedule_minutes;
        self.file_config = request.file_config;
        self.column_mappings = request.column_mappings;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JobCursor {
    #[serde(default)]
    pub last_pushed_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_pushed_row_index: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLogEntry {
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersistedDatasource {
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub file_path: String,
    #[serde(default = "default_schedule_minutes")]
    pub schedule_minutes: u32,
    pub file_config: FileConfig,
    #[serde(default)]
    pub column_mappings: Vec<ColumnMapping>,
    #[serde(default)]
    pub last_pushed_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_pushed_row_index: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub recent_logs: Vec<JobLogEntry>,
}

impl PersistedDatasource {
    pub fn to_job_config(&self) -> JobConfig {
        JobConfig {
            id: self.id.clone(),
            name: self.name.clone(),
            enabled: self.enabled,
            file_path: self.file_path.clone(),
            schedule_minutes: self.schedule_minutes,
            file_config: self.file_config.clone(),
            column_mappings: self.column_mappings.clone(),
        }
    }

    pub fn to_cursor(&self) -> JobCursor {
        JobCursor {
            last_pushed_timestamp: self.last_pushed_timestamp,
            last_pushed_row_index: self.last_pushed_row_index,
            last_run_at: self.last_run_at,
            last_error: self.last_error.clone(),
        }
    }

    pub fn from_job(
        job: JobConfig,
        cursor: Option<JobCursor>,
        recent_logs: Option<Vec<JobLogEntry>>,
    ) -> Self {
        let cursor = cursor.unwrap_or_default();
        Self {
            id: job.id,
            name: job.name,
            enabled: job.enabled,
            file_path: job.file_path,
            schedule_minutes: job.schedule_minutes,
            file_config: job.file_config,
            column_mappings: job.column_mappings,
            last_pushed_timestamp: cursor.last_pushed_timestamp,
            last_pushed_row_index: cursor.last_pushed_row_index,
            last_run_at: cursor.last_run_at,
            last_error: cursor.last_error,
            recent_logs: recent_logs.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub jobs: Vec<JobConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            server: ServerConfig::default(),
            jobs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppStateFile {
    #[serde(default)]
    pub cursors: HashMap<String, JobCursor>,
    #[serde(default)]
    pub logs: HashMap<String, Vec<JobLogEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceStateFile {
    #[serde(default = "default_version")]
    pub version: u32,
    pub workspace_id: String,
    #[serde(default)]
    pub workspace_name: String,
    #[serde(default)]
    pub hydroserver_url: String,
    #[serde(default)]
    pub datasources: Vec<PersistedDatasource>,
}

impl Default for WorkspaceStateFile {
    fn default() -> Self {
        Self {
            version: default_version(),
            workspace_id: String::new(),
            workspace_name: String::new(),
            hydroserver_url: String::new(),
            datasources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub config_dir: String,
    pub server_configured: bool,
    pub connection: ConnectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionTestResponse {
    pub ok: bool,
    pub state: ConnectionState,
    pub message: String,
    #[serde(default)]
    pub instance_name: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub workspace_name: Option<String>,
    #[serde(default)]
    pub workspace_count: u32,
    #[serde(default)]
    pub datastream_count: u32,
    #[serde(default)]
    pub permissions_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerUrlValidationResponse {
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub instance_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResponse {
    #[serde(default = "default_true")]
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub thing_id: String,
    #[serde(default)]
    pub thing_name: String,
    #[serde(default)]
    pub observed_property_name: String,
    #[serde(default)]
    pub processing_level_definition: String,
    #[serde(default)]
    pub unit_name: String,
    #[serde(default)]
    pub unit_symbol: String,
    #[serde(default)]
    pub sampled_medium: String,
    #[serde(default)]
    pub sensor_name: String,
    #[serde(default)]
    pub result_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamThingLocationDetail {
    #[serde(default)]
    pub latitude: String,
    #[serde(default)]
    pub longitude: String,
    #[serde(default)]
    pub elevation_m: String,
    #[serde(default)]
    pub elevation_datum: String,
    #[serde(default)]
    pub admin_area_1: String,
    #[serde(default)]
    pub admin_area_2: String,
    #[serde(default)]
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamThingDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sampling_feature_code: String,
    #[serde(default)]
    pub site_type: String,
    #[serde(default)]
    pub sampling_feature_type: String,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub location: DatastreamThingLocationDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamObservedPropertyDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub definition: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub property_type: String,
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamUnitDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub definition: String,
    #[serde(default)]
    pub unit_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamSensorDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub method_type: String,
    #[serde(default)]
    pub method_code: String,
    #[serde(default)]
    pub method_link: String,
    #[serde(default)]
    pub encoding_type: String,
    #[serde(default)]
    pub model_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamProcessingLevelDetail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub definition: String,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatastreamDetail {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sampled_medium: String,
    #[serde(default)]
    pub result_type: String,
    #[serde(default)]
    pub observation_type: String,
    #[serde(default)]
    pub no_data_value: String,
    #[serde(default)]
    pub aggregation_statistic: String,
    #[serde(default)]
    pub intended_time_spacing: String,
    #[serde(default)]
    pub intended_time_spacing_unit: String,
    #[serde(default)]
    pub time_aggregation_interval: String,
    #[serde(default)]
    pub time_aggregation_interval_unit: String,
    #[serde(default)]
    pub phenomenon_begin_time: String,
    #[serde(default)]
    pub phenomenon_end_time: String,
    #[serde(default)]
    pub value_count: String,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_visible: bool,
    #[serde(default)]
    pub thing: DatastreamThingDetail,
    #[serde(default)]
    pub observed_property: DatastreamObservedPropertyDetail,
    #[serde(default)]
    pub unit: DatastreamUnitDetail,
    #[serde(default)]
    pub sensor: DatastreamSensorDetail,
    #[serde(default)]
    pub processing_level: DatastreamProcessingLevelDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStatusSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub file_path: String,
    pub schedule_minutes: u32,
    pub file_config: FileConfig,
    pub column_mappings: Vec<ColumnMapping>,
    pub status: JobStatus,
    pub status_message: String,
    #[serde(default)]
    pub last_pushed_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDetail {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub file_path: String,
    pub schedule_minutes: u32,
    pub file_config: FileConfig,
    pub column_mappings: Vec<ColumnMapping>,
    pub status: JobStatus,
    pub status_message: String,
    #[serde(default)]
    pub last_pushed_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub recent_logs: Vec<JobLogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CsvPreviewResponse {
    pub raw_lines: Vec<String>,
    pub parsed_rows: Vec<Vec<String>>,
    pub detected_header_row: Option<u32>,
    pub detected_data_start_row: Option<u32>,
    pub detected_delimiter: String,
    pub total_lines: usize,
    pub encoding: String,
}

pub fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn default_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_enabled() -> bool {
    true
}

fn default_schedule_minutes() -> u32 {
    15
}

fn default_timestamp_key() -> String {
    "timestamp".to_string()
}

fn default_header_row() -> u32 {
    1
}

fn default_optional_header_row() -> Option<u32> {
    Some(default_header_row())
}

fn default_data_start_row() -> u32 {
    2
}

fn default_delimiter() -> String {
    ",".to_string()
}
