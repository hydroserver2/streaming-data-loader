import { apiBaseUrl } from "./config"

export type ConnectionState = "not_configured" | "configured" | "connected" | "error"
export type JobStatus = "healthy" | "warning" | "error" | "disabled" | "pending" | "running"

export interface ServerConfig {
  url: string
  api_key: string
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
      const payload = (await response.json()) as { detail?: string }
      if (payload.detail) {
        detail = payload.detail
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

export function testConnection(server: ServerConfig): Promise<ConnectionTestResponse> {
  return request<ConnectionTestResponse>("/connection/test", {
    method: "POST",
    body: JSON.stringify(server),
  })
}

export function listJobs(): Promise<JobSummary[]> {
  return request<JobSummary[]>("/jobs")
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
