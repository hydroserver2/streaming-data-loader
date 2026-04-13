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
  status: "healthy" | "warning" | "error" | "disabled" | "pending" | "running"
  status_message: string
  last_pushed_timestamp: string | null
  last_run_at: string | null
  last_error: string | null
}

function buildApiUrl(path: string): string {
  return `${apiBaseUrl.replace(/\/$/, "")}${path}`
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

export function getHealth(): Promise<HealthResponse> {
  return request<HealthResponse>("/health")
}

export function getConfig(): Promise<AppConfig> {
  return request<AppConfig>("/config")
}

export function updateServerConfig(server: ServerConfig): Promise<AppConfig> {
  return request<AppConfig>("/config/server", {
    method: "PUT",
    body: JSON.stringify(server),
  })
}

export function clearServerConfig(): Promise<AppConfig> {
  return request<AppConfig>("/config/server", {
    method: "DELETE",
  })
}

export function testConnection(server: ServerConfig): Promise<ConnectionTestResponse> {
  return request<ConnectionTestResponse>("/connection/test", {
    method: "POST",
    body: JSON.stringify(server),
  })
}

export function validateServerUrl(url: string): Promise<ServerUrlValidationResponse> {
  const params = new URLSearchParams({ url })
  return request<ServerUrlValidationResponse>(
    `/connection/validate-url?${params.toString()}`
  )
}

export function getCsvPreview(path: string, rows = 100): Promise<CsvPreviewResponse> {
  const params = new URLSearchParams({
    path,
    rows: String(rows),
  })
  return request<CsvPreviewResponse>(`/csv/preview?${params.toString()}`)
}

export function getDatastreams(): Promise<DatastreamSummary[]> {
  return request<DatastreamSummary[]>("/datastreams")
}

export function createJob(payload: JobUpsertRequest): Promise<JobDetail> {
  return request<JobDetail>("/jobs", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateJob(jobId: string, payload: JobUpsertRequest): Promise<JobDetail> {
  return request<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function getJob(jobId: string): Promise<JobDetail> {
  return request<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`)
}

export function getJobLogs(jobId: string): Promise<JobLogEntry[]> {
  return request<JobLogEntry[]>(`/jobs/${encodeURIComponent(jobId)}/logs`)
}
