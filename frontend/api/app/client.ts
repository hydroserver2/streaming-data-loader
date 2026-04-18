import { requestJson } from "../http"
import { invokeCommand, isTauriRuntime } from "../runtime"
import { daemonCommand, subscribeToDaemonStatus } from "./daemonTransport"
import type {
  ActionResponse,
  AppBootstrapResponse,
  AppConfig,
  ConnectionTestResponse,
  CsvPreviewResponse,
  DaemonStatusSnapshot,
  DatastreamDetail,
  DatastreamSummary,
  HealthResponse,
  JobDetail,
  JobLogsResponse,
  JobStatusSummary,
  JobUpsertRequest,
  ServerConfig,
  ServerUrlValidationResponse,
} from "./types"

export { subscribeToDaemonStatus }
export type { DaemonStatusSnapshot }

export function getBootstrap(): Promise<AppBootstrapResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<AppBootstrapResponse>("bootstrap")
  }

  return Promise.all([getHealth(), getConfig(), getJobs()]).then(([health, config, jobs]) => ({
    health,
    config,
    jobs,
  }))
}

export function getHealth(): Promise<HealthResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<HealthResponse>("get-health")
  }
  return requestJson<HealthResponse>("/health")
}

export function getConfig(): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return daemonCommand<AppConfig>("get-config")
  }
  return requestJson<AppConfig>("/config")
}

export function updateServerConfig(server: ServerConfig): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return daemonCommand<AppConfig>("update-server-config", { server })
  }
  return requestJson<AppConfig>("/config/server", {
    method: "PUT",
    body: JSON.stringify(server),
  })
}

export function clearServerConfig(): Promise<AppConfig> {
  if (isTauriRuntime()) {
    return daemonCommand<AppConfig>("clear-server-config")
  }
  return requestJson<AppConfig>("/config/server", {
    method: "DELETE",
  })
}

export function testConnection(server: ServerConfig): Promise<ConnectionTestResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ConnectionTestResponse>("test-connection", { server })
  }
  return requestJson<ConnectionTestResponse>("/connection/test", {
    method: "POST",
    body: JSON.stringify(server),
  })
}

export function validateServerUrl(url: string): Promise<ServerUrlValidationResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ServerUrlValidationResponse>("validate-server-url", { url })
  }
  const params = new URLSearchParams({ url })
  return requestJson<ServerUrlValidationResponse>(
    `/connection/validate-url?${params.toString()}`
  )
}

export function getCsvPreview(path: string, rows = 100): Promise<CsvPreviewResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<CsvPreviewResponse>("get-csv-preview", { path, rows })
  }
  const params = new URLSearchParams({
    path,
    rows: String(rows),
  })
  return requestJson<CsvPreviewResponse>(`/csv/preview?${params.toString()}`)
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
    return daemonCommand<DatastreamSummary[]>("get-datastreams")
  }
  return requestJson<DatastreamSummary[]>("/datastreams")
}

export function getDatastreamDetail(datastreamId: string): Promise<DatastreamDetail> {
  if (isTauriRuntime()) {
    return daemonCommand<DatastreamDetail>("get-datastream-detail", {
      datastream_id: datastreamId,
    })
  }

  return Promise.reject(
    new Error("Expanded datastream metadata is only available in the desktop app.")
  )
}

export function createJob(payload: JobUpsertRequest): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return daemonCommand<JobDetail>("create-job", { payload })
  }
  return requestJson<JobDetail>("/jobs", {
    method: "POST",
    body: JSON.stringify(payload),
  })
}

export function updateJob(jobId: string, payload: JobUpsertRequest): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return daemonCommand<JobDetail>("update-job", { job_id: jobId, payload })
  }
  return requestJson<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  })
}

export function getJob(jobId: string): Promise<JobDetail> {
  if (isTauriRuntime()) {
    return daemonCommand<JobDetail>("get-job", { job_id: jobId })
  }
  return requestJson<JobDetail>(`/jobs/${encodeURIComponent(jobId)}`)
}

export function getJobLogs(jobId: string): Promise<JobLogsResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<JobLogsResponse>("get-job-logs", { job_id: jobId })
  }
  return requestJson<JobLogsResponse>(`/jobs/${encodeURIComponent(jobId)}/logs`)
}

export function getJobs(): Promise<JobStatusSummary[]> {
  if (isTauriRuntime()) {
    return daemonCommand<JobStatusSummary[]>("get-jobs")
  }
  return requestJson<JobStatusSummary[]>("/jobs")
}

export function deleteJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ActionResponse>("delete-job", { job_id: jobId })
  }
  return requestJson<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}`, {
    method: "DELETE",
  })
}

export function runJobNow(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ActionResponse>("run-job-now", { job_id: jobId })
  }
  return requestJson<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/run`, {
    method: "POST",
  })
}

export function enableJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ActionResponse>("enable-job", { job_id: jobId })
  }
  return requestJson<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/enable`, {
    method: "POST",
  })
}

export function disableJob(jobId: string): Promise<ActionResponse> {
  if (isTauriRuntime()) {
    return daemonCommand<ActionResponse>("disable-job", { job_id: jobId })
  }
  return requestJson<ActionResponse>(`/jobs/${encodeURIComponent(jobId)}/disable`, {
    method: "POST",
  })
}
