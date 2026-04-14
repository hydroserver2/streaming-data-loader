import { apiBaseUrl } from "./config"
import type { Timestamp } from "./models/timestamp"

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

function buildApiUrl(path: string): string {
  return `${apiBaseUrl.replace(/\/$/, "")}${path}`
}

function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as Window & typeof globalThis)
  )
}

function formatErrorDetail(detail: unknown): string | null {
  if (typeof detail === "string" && detail.trim()) {
    return detail
  }

  if (Array.isArray(detail)) {
    const firstMessage = detail
      .map((item) => {
        if (typeof item === "string") return item
        if (
          item &&
          typeof item === "object" &&
          "msg" in item &&
          typeof item.msg === "string"
        ) {
          return item.msg
        }
        return null
      })
      .find(Boolean)

    return firstMessage ?? null
  }

  if (detail && typeof detail === "object") {
    if ("msg" in detail && typeof detail.msg === "string") {
      return detail.msg
    }

    try {
      return JSON.stringify(detail)
    } catch {
      return null
    }
  }

  return null
}

function normalizeError(error: unknown): Error {
  if (error instanceof Error) return error
  if (typeof error === "string" && error.trim()) return new Error(error)
  return new Error("Request failed.")
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(buildApiUrl(path), {
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  })

  if (!response.ok) {
    let detail = `Request failed with status ${response.status}`

    try {
      const payload = (await response.json()) as { detail?: unknown }
      const formattedDetail = formatErrorDetail(payload.detail)
      if (formattedDetail) {
        detail = formattedDetail
      }
    } catch {
      // Ignore JSON parsing errors for non-JSON error responses.
    }

    throw new Error(detail)
  }

  return (await response.json()) as T
}

async function invokeCommand<T>(
  command: string,
  payload?: Record<string, unknown>
): Promise<T> {
  try {
    const { invoke } = await import("@tauri-apps/api/core")
    return await invoke<T>(command, payload)
  } catch (error) {
    throw normalizeError(error)
  }
}

export function getHealth(): Promise<HealthResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<HealthResponse>("get_health")
  }
  return request<HealthResponse>("/health")
}

export function getConfig(): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return invokeCommand<AppConfig>("get_config")
  }
  return request<AppConfig>("/config")
}

export function updateServerConfig(server: ServerConfig): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return invokeCommand<AppConfig>("update_server_config", { server })
  }
  return request<AppConfig>("/config/server", {
    method: "PUT",
    body: JSON.stringify(server),
  })
}

export function clearServerConfig(): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return invokeCommand<AppConfig>("clear_server_config")
  }
  return request<AppConfig>("/config/server", {
    method: "DELETE",
  })
}

export function testConnection(server: ServerConfig): Promise<ConnectionTestResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ConnectionTestResponse>("test_connection", { server })
  }
  return request<ConnectionTestResponse>("/connection/test", {
    method: "POST",
    body: JSON.stringify(server),
  })
}

export function validateServerUrl(url: string): Promise<ServerUrlValidationResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ServerUrlValidationResponse>("validate_server_url", { url })
  }
  const params = new URLSearchParams({ url })
  return request<ServerUrlValidationResponse>(
    `/connection/validate-url?${params.toString()}`
  )
}

export function getCsvPreview(path: string, rows = 100): Promise<CsvPreviewResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<CsvPreviewResponse>("get_csv_preview", { path, rows })
  }
  const params = new URLSearchParams({
    path,
    rows: String(rows),
  })
  return request<CsvPreviewResponse>(`/csv/preview?${params.toString()}`)
}

export function revealFileInFolder(path: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ActionResponse>("reveal_file_in_folder", { path })
  }

  return Promise.reject(
    new Error("Opening the local file system is only available in the desktop app.")
  )
}

export function getDatastreams(): Promise<DatastreamSummary[]> {
  if (isTauriRuntime()) {
    return invokeCommand<DatastreamSummary[]>("get_datastreams")
  }
  return request<DatastreamSummary[]>("/datastreams")
}

export function getDatastreamDetail(datastreamId: string): Promise<DatastreamDetail> {
  if (isTauriRuntime()) {
    return invokeCommand<DatastreamDetail>("get_datastream_detail", {
      datastreamId,
    })
  }

  return Promise.reject(
    new Error("Expanded datastream metadata is only available in the desktop app.")
  )
}

export function createJob(payload: JobUpsertRequest): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return invokeCommand<JobDetail>("create_job", { payload })
  }
  return request<JobDetail>("/jobs", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateJob(jobId: string, payload: JobUpsertRequest): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return invokeCommand<JobDetail>("update_job", { jobId, payload })
  }
  return request<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function getJob(jobId: string): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return invokeCommand<JobDetail>("get_job", { jobId })
  }
  return request<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`)
}

export function getJobLogs(jobId: string): Promise<JobLogEntry[]> {
  if (isTauriRuntime()) {
    return invokeCommand<JobLogEntry[]>("get_job_logs", { jobId })
  }
  return request<JobLogEntry[]>(`/jobs/${encodeURIComponent(jobId)}/logs`)
}

export function getJobs(): Promise<JobStatusSummary[]> {
  if (isTauriRuntime()) {
    return invokeCommand<JobStatusSummary[]>("get_jobs")
  }
  return request<JobStatusSummary[]>("/jobs")
}

export interface ActionResponse {
  ok: boolean
  message: string
}

export function deleteJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ActionResponse>("delete_job", { jobId })
  }
  return request<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}`, {
    method: "DELETE",
  })
}

export function runJobNow(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ActionResponse>("run_job_now", { jobId })
  }
  return request<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/run`, {
    method: "POST",
  })
}

export function enableJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ActionResponse>("enable_job", { jobId })
  }
  return request<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/enable`, {
    method: "POST",
  })
}

export function disableJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ActionResponse>("disable_job", { jobId })
  }
  return request<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/disable`, {
    method: "POST",
  })
}
