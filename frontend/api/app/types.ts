import type { Timestamp } from "../../models/timestamp"

export type ConnectionState = "not_configured" | "configured" | "connected" | "error"
export type AuthType = "apikey" | "userpass"

export interface ServerConfig {
  auth_type: AuthType
  url: string
  api_key: string
  username: string
  password: string
  workspace_id: string
  workspace_name: string
}

export interface AppConfig {
  version: number
  server: ServerConfig
  jobs: JobConfig[]
}

export interface ConnectionStatus {
  state: ConnectionState
  message: string
}

export interface HealthResponse {
  status: "ok"
  version: string
  config_dir: string
  server_configured: boolean
  connection: ConnectionStatus
}

export interface AppBootstrapResponse {
  health: HealthResponse
  config: AppConfig
  jobs: JobStatusSummary[]
}

export interface DaemonConnectionInfo {
  base_url: string
  token: string
}

export interface DaemonStatusSnapshot {
  health: HealthResponse
  config: AppConfig
  jobs: JobStatusSummary[]
}

export interface ConnectionTestResponse {
  ok: boolean
  state: ConnectionState
  message: string
  invalid_field: string | null
  instance_name: string | null
  workspace_id: string | null
  workspace_name: string | null
  workspace_count: number
  datastream_count: number
  permissions_ok: boolean
}

export interface ServerUrlValidationResponse {
  ok: boolean
  message: string
  instance_name: string | null
}

export interface CsvPreviewResponse {
  raw_lines: string[]
  parsed_rows: string[][]
  detected_header_row: number | null
  detected_data_start_row: number | null
  detected_delimiter: string
  total_lines: number
  encoding: string
}

export type CsvTransformerIdentifierType = "name" | "index"
export interface CsvTransformerTimestampSettings extends Timestamp {
  key: string
}

export interface CsvTransformerSettings {
  headerRow: number | null
  dataStartRow: number
  delimiter: string
  identifierType: CsvTransformerIdentifierType
  timestamp: CsvTransformerTimestampSettings
}

export interface DatastreamSummary {
  id: string
  name: string
  thing_id: string
  thing_name: string
  observed_property_name: string
  processing_level_definition: string
  unit_name: string
  unit_symbol: string
  sampled_medium: string
  sensor_name: string
  result_type: string
}

export interface DatastreamThingLocationDetail {
  latitude: string
  longitude: string
  elevation_m: string
  elevation_datum: string
  admin_area_1: string
  admin_area_2: string
  country: string
}

export interface DatastreamThingDetail {
  id: string
  name: string
  description: string
  sampling_feature_code: string
  site_type: string
  sampling_feature_type: string
  is_private: boolean
  location: DatastreamThingLocationDetail
}

export interface DatastreamObservedPropertyDetail {
  id: string
  name: string
  definition: string
  description: string
  property_type: string
  code: string
}

export interface DatastreamUnitDetail {
  id: string
  name: string
  symbol: string
  definition: string
  unit_type: string
}

export interface DatastreamSensorDetail {
  id: string
  name: string
  description: string
  manufacturer: string
  model: string
  method_type: string
  method_code: string
  method_link: string
  encoding_type: string
  model_link: string
}

export interface DatastreamProcessingLevelDetail {
  id: string
  code: string
  definition: string
  explanation: string
}

export interface DatastreamDetail {
  id: string
  name: string
  description: string
  sampled_medium: string
  result_type: string
  observation_type: string
  no_data_value: string
  aggregation_statistic: string
  intended_time_spacing: string
  intended_time_spacing_unit: string
  time_aggregation_interval: string
  time_aggregation_interval_unit: string
  phenomenon_begin_time: string
  phenomenon_end_time: string
  value_count: string
  is_private: boolean
  is_visible: boolean
  thing: DatastreamThingDetail
  observed_property: DatastreamObservedPropertyDetail
  unit: DatastreamUnitDetail
  sensor: DatastreamSensorDetail
  processing_level: DatastreamProcessingLevelDetail
}

export interface ColumnMapping {
  csv_column: string
  datastream_id: string
  datastream_name: string
}

export interface JobConfig {
  id: string
  name: string
  enabled: boolean
  file_path: string
  schedule_minutes: number
  file_config: CsvTransformerSettings
  column_mappings: ColumnMapping[]
}

export interface JobLogEntry {
  timestamp: string
  level: "info" | "warning" | "error"
  message: string
}

export interface JobLogsResponse {
  entries: JobLogEntry[]
  log_file_path: string | null
}

export type JobStatus =
  | "healthy"
  | "warning"
  | "error"
  | "disabled"
  | "pending"
  | "running"

export interface JobStatusSummary {
  id: string
  name: string
  enabled: boolean
  file_path: string
  schedule_minutes: number
  file_config: CsvTransformerSettings
  column_mappings: ColumnMapping[]
  status: JobStatus
  status_message: string
  last_pushed_timestamp: string | null
  last_run_at: string | null
  last_error: string | null
}

export interface JobUpsertRequest {
  name: string
  enabled?: boolean
  file_path: string
  schedule_minutes?: number
  file_config: CsvTransformerSettings
  column_mappings: ColumnMapping[]
}

export interface JobDetail extends JobUpsertRequest {
  id: string
  enabled: boolean
  schedule_minutes: number
  recent_logs: JobLogEntry[]
  status: JobStatus
  status_message: string
  last_pushed_timestamp: string | null
  last_run_at: string | null
  last_error: string | null
}

export interface ActionResponse {
  ok: boolean
  message: string
}
