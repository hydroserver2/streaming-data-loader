import axios from "axios"

const client = axios.create({
  baseURL: "http://127.0.0.1:5321",
  headers: { "Content-Type": "application/json" },
})

// ── Types ─────────────────────────────────────────────────────────────────────

export interface HydroServerConnection {
  id: string
  name: string
  host: string
  auth_type: "apikey" | "userpass"
  api_key: string | null
  username: string | null
  password: string | null
}

export interface ConnectionPayload {
  name: string
  host: string
  auth_type: "apikey" | "userpass"
  api_key?: string | null
  username?: string | null
  password?: string | null
}

export interface Schedule {
  period: "days" | "hours" | "minutes"
  interval: number
  start_time: string
}

export interface ColumnMapping {
  csv_column: string
  datastream_id: string
}

export interface Task {
  id: string
  name: string
  connection_id: string
  schedule: Schedule | null
  is_active: boolean
  source_type: "http" | "local"
  file_path: string
  csv_delimiter: string
  csv_header_row: number
  csv_timestamp_column: string
  csv_timestamp_format: string
  column_mappings: ColumnMapping[]
  latest_run?: TaskRun | null
}

export interface TaskPayload {
  name: string
  connection_id: string
  schedule?: Schedule | null
  is_active?: boolean
  source_type: "http" | "local"
  file_path: string
  csv_delimiter?: string
  csv_header_row?: number
  csv_timestamp_column: string
  csv_timestamp_format: string
  column_mappings?: ColumnMapping[]
}

export interface TaskRun {
  id: string
  task_id: string
  status: "started" | "success" | "failure"
  started_at: string
  completed_at: string | null
  error_message: string | null
  success_count: number | null
  failure_count: number | null
  skipped_count: number | null
  values_loaded_total: number | null
  earliest_timestamp: string | null
  latest_timestamp: string | null
}

// ── Connections ───────────────────────────────────────────────────────────────

export const api = {
  connections: {
    list: () =>
      client.get<HydroServerConnection[]>("/connections/").then(r => r.data),

    get: (id: string) =>
      client.get<HydroServerConnection>(`/connections/${id}`).then(r => r.data),

    create: (payload: ConnectionPayload) =>
      client.post<HydroServerConnection>("/connections/", payload).then(r => r.data),

    update: (id: string, payload: ConnectionPayload) =>
      client.put<HydroServerConnection>(`/connections/${id}`, payload).then(r => r.data),

    delete: (id: string) =>
      client.delete(`/connections/${id}`).then(r => r.data),
  },

  // ── Tasks ──────────────────────────────────────────────────────────────────

  tasks: {
    list: () =>
      client.get<Task[]>("/tasks/").then(r => r.data),

    get: (id: string) =>
      client.get<Task>(`/tasks/${id}`).then(r => r.data),

    create: (payload: TaskPayload) =>
      client.post<Task>("/tasks/", payload).then(r => r.data),

    update: (id: string, payload: TaskPayload) =>
      client.put<Task>(`/tasks/${id}`, payload).then(r => r.data),

    delete: (id: string) =>
      client.delete(`/tasks/${id}`).then(r => r.data),

    runNow: (id: string) =>
      client.post(`/tasks/${id}/run`).then(r => r.data),
  },

  // ── Runs ───────────────────────────────────────────────────────────────────

  runs: {
    list: (taskId?: string) =>
      client.get<TaskRun[]>("/runs/", {
        params: taskId ? { task_id: taskId } : undefined,
      }).then(r => r.data),

    get: (id: string) =>
      client.get<TaskRun>(`/runs/${id}`).then(r => r.data),
  },
}
