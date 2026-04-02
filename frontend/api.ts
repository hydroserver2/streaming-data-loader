import { apiBaseUrl } from "./config"

export type ConnectionState = "not_configured" | "configured" | "connected" | "error"
export type JobStatus = "healthy" | "warning" | "error" | "disabled" | "pending" | "running"
export type AuthType = "apikey" | "userpass"

export interface ServerConfig {
  auth_type: AuthType
  url: string
  api_key: string
  username: string
  password: string
}

export interface FileConfig {
  header_row: number
  data_start_row: number
  delimiter: string
  timestamp_column: string
  timestamp_format: string
  timezone: string
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
  file_config: FileConfig
  column_mappings: ColumnMapping[]
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
  workspace_count: number
  datastream_count: number
  permissions_ok: boolean
}

export interface DatastreamSummary {
  id: string
  name: string
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

export interface JobSummary extends JobConfig {
  status: JobStatus
  status_message: string
  last_pushed_timestamp: string | null
  last_run_at: string | null
  last_error: string | null
}

export interface JobLogEntry {
  timestamp: string
  level: "info" | "warning" | "error"
  message: string
}

export interface JobDetail extends JobSummary {
  recent_logs: JobLogEntry[]
}

export interface ActionResponse {
  ok: boolean
  message: string
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
      .map(item => {
        if (typeof item === "string") {
          return item
        }
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

export function listJobs(): Promise<JobSummary[]> {
  return request<JobSummary[]>("/jobs")
}

export function createJob(job: Omit<JobConfig, "id">): Promise<JobDetail> {
  return request<JobDetail>("/jobs", {
    method: "POST",
    body: JSON.stringify(job),
  })
}

export function getDatastreams(): Promise<DatastreamSummary[]> {
  return request<DatastreamSummary[]>("/datastreams")
}

export function getCsvPreview(path: string, rows = 60): Promise<CsvPreviewResponse> {
  const params = new URLSearchParams({
    path,
    rows: String(rows),
  })
  return request<CsvPreviewResponse>(`/csv/preview?${params.toString()}`)
}

export function runJob(jobId: string): Promise<ActionResponse> {
  return request<ActionResponse>(`/jobs/${jobId}/run`, {
    method: "POST",
  })
}

export function enableJob(jobId: string): Promise<ActionResponse> {
  return request<ActionResponse>(`/jobs/${jobId}/enable`, {
    method: "POST",
  })
}

export function disableJob(jobId: string): Promise<ActionResponse> {
  return request<ActionResponse>(`/jobs/${jobId}/disable`, {
    method: "POST",
  })
}

export function deleteJob(jobId: string): Promise<ActionResponse> {
  return request<ActionResponse>(`/jobs/${jobId}`, {
    method: "DELETE",
  })
}
