<template>
  <div class="flex flex-col h-full">

    <!-- Top header bar -->
    <div class="flex items-center justify-between px-6 py-4 border-b border-base-300 bg-base-100">
      <div class="flex items-center gap-3">
        <button class="btn btn-ghost btn-sm -ml-2" @click="router.push({ name: 'tasks' })">
          ← Back
        </button>
        <div v-if="task">
          <h2 class="text-base font-semibold text-base-content leading-tight">{{ task.name }}</h2>
          <p class="text-xs text-base-content/50">{{ formatSchedule(task.schedule) }}</p>
        </div>
        <div v-else-if="loading" class="h-8 w-40 bg-base-200 rounded animate-pulse" />
      </div>
      <div v-if="task" class="flex items-center gap-2">
        <button
          class="btn btn-ghost btn-sm"
          :class="{ loading: running }"
          :disabled="running"
          @click="runNow"
        >
          Run Now
        </button>
        <button class="btn btn-ghost btn-sm" @click="openEdit">Edit</button>
        <label class="flex items-center gap-2 text-sm text-base-content/60 cursor-pointer">
          <input
            type="checkbox"
            class="toggle toggle-sm toggle-primary"
            :checked="task.is_active"
            @change="toggleActive"
          />
          <span>{{ task.is_active ? "Active" : "Inactive" }}</span>
        </label>
      </div>
    </div>

    <!-- Loading / not found -->
    <div v-if="loading" class="flex justify-center py-16">
      <span class="loading loading-spinner loading-md text-primary" />
    </div>
    <div v-else-if="!task" class="text-center py-16 text-base-content/50 text-sm">
      Task not found.
    </div>

    <template v-else>

      <!-- Tabs -->
      <div class="px-6 border-b border-base-300 bg-base-100">
        <div role="tablist" class="tabs tabs-bordered -mb-px">
          <button
            v-for="tab in tabs"
            :key="tab.key"
            role="tab"
            class="tab text-sm"
            :class="activeTab === tab.key ? 'tab-active font-medium' : 'text-base-content/60'"
            @click="activeTab = tab.key"
          >
            {{ tab.label }}
          </button>
        </div>
      </div>

      <!-- Tab: Details -->
      <div v-if="activeTab === 'details'" class="p-6 overflow-y-auto">
        <div class="max-w-3xl mx-auto grid grid-cols-2 gap-4">

          <div class="border border-base-300 rounded-xl p-4">
            <h4 class="text-xs font-medium text-base-content/50 uppercase tracking-wide mb-3">Connection</h4>
            <dl class="flex flex-col gap-2">
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Name</dt>
                <dd class="font-medium">{{ connection?.name ?? "—" }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Host</dt>
                <dd class="font-medium truncate max-w-[180px]">{{ connection?.host ?? "—" }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Auth</dt>
                <dd>
                  <span class="badge badge-sm badge-ghost">
                    {{ connection?.auth_type === "apikey" ? "API Key" : "User/Pass" }}
                  </span>
                </dd>
              </div>
            </dl>
          </div>

          <div class="border border-base-300 rounded-xl p-4">
            <h4 class="text-xs font-medium text-base-content/50 uppercase tracking-wide mb-3">Source</h4>
            <dl class="flex flex-col gap-2">
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Type</dt>
                <dd>
                  <span class="badge badge-sm badge-ghost">
                    {{ task.source_type === "http" ? "Remote URL" : "Local File" }}
                  </span>
                </dd>
              </div>
              <div class="flex flex-col gap-1 text-sm">
                <dt class="text-base-content/60">Path</dt>
                <dd class="font-mono text-xs bg-base-200 rounded px-2 py-1 break-all">{{ task.file_path }}</dd>
              </div>
            </dl>
          </div>

          <div class="border border-base-300 rounded-xl p-4">
            <h4 class="text-xs font-medium text-base-content/50 uppercase tracking-wide mb-3">CSV Settings</h4>
            <dl class="flex flex-col gap-2">
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Delimiter</dt>
                <dd class="font-mono">{{ task.csv_delimiter }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Header Row</dt>
                <dd class="font-mono">{{ task.csv_header_row }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Timestamp Column</dt>
                <dd class="font-mono">{{ task.csv_timestamp_column }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Timestamp Format</dt>
                <dd class="font-mono text-xs">{{ task.csv_timestamp_format }}</dd>
              </div>
            </dl>
          </div>

          <div class="border border-base-300 rounded-xl p-4">
            <h4 class="text-xs font-medium text-base-content/50 uppercase tracking-wide mb-3">Schedule</h4>
            <dl class="flex flex-col gap-2">
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Interval</dt>
                <dd class="font-medium">{{ formatSchedule(task.schedule) }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Start Time</dt>
                <dd class="font-mono">{{ task.schedule?.start_time ?? "—" }}</dd>
              </div>
              <div class="flex justify-between text-sm">
                <dt class="text-base-content/60">Active</dt>
                <dd>
                  <span class="badge badge-sm" :class="task.is_active ? 'badge-success' : 'badge-ghost'">
                    {{ task.is_active ? "Yes" : "No" }}
                  </span>
                </dd>
              </div>
            </dl>
          </div>

        </div>
      </div>

      <!-- Tab: Mappings -->
      <div v-else-if="activeTab === 'mappings'" class="p-6 overflow-y-auto">
        <div class="max-w-3xl mx-auto border border-base-300 rounded-xl overflow-hidden">
          <table v-if="task.column_mappings && task.column_mappings.length > 0" class="table table-sm w-full">
            <thead class="bg-base-200 text-base-content/60 text-xs uppercase tracking-wide">
              <tr>
                <th class="font-medium">CSV Column</th>
                <th class="font-medium">Datastream ID</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(mapping, i) in task.column_mappings"
                :key="i"
                class="border-t border-base-300"
              >
                <td class="font-mono text-sm">{{ mapping.csv_column }}</td>
                <td class="font-mono text-sm text-base-content/70">{{ mapping.datastream_id }}</td>
              </tr>
            </tbody>
          </table>
          <p v-else class="text-sm text-base-content/40 px-4 py-6 text-center">
            No column mappings configured.
          </p>
        </div>
      </div>

      <!-- Tab: Runs -->
      <div v-else-if="activeTab === 'runs'" class="p-6 overflow-y-auto flex flex-col gap-4">
        <div class="max-w-5xl mx-auto w-full flex flex-col gap-4">

          <!-- Filters -->
          <div class="flex items-center gap-3">
            <input
              v-model="search"
              type="text"
              class="input input-bordered input-sm flex-1"
              placeholder="Search runs..."
              @input="currentPage = 1"
            />
            <select
              v-model="statusFilter"
              class="select select-bordered select-sm w-36"
              @change="currentPage = 1"
            >
              <option value="">All statuses</option>
              <option value="started">Started</option>
              <option value="success">Success</option>
              <option value="failure">Failure</option>
            </select>
            <select
              v-model="pageSize"
              class="select select-bordered select-sm w-24"
              @change="currentPage = 1"
            >
              <option :value="10">10 / page</option>
              <option :value="25">25 / page</option>
              <option :value="50">50 / page</option>
            </select>
          </div>

          <!-- Loading -->
          <div v-if="runsLoading && runs.length === 0" class="flex justify-center py-10">
            <span class="loading loading-spinner loading-sm text-primary" />
          </div>

          <template v-else>
            <div class="border border-base-300 rounded-xl overflow-hidden">
              <table class="table table-sm w-full">
                <thead class="bg-base-200 text-base-content/60 text-xs uppercase tracking-wide">
                  <tr>
                    <th
                      v-for="col in columns"
                      :key="col.key"
                      class="font-medium cursor-pointer select-none hover:text-base-content transition-colors"
                      @click="setSort(col.key)"
                    >
                      <div class="flex items-center gap-1">
                        {{ col.label }}
                        <span class="text-base-content/30">
                          <template v-if="sortKey === col.key">
                            {{ sortDir === "asc" ? "↑" : "↓" }}
                          </template>
                          <template v-else>↕</template>
                        </span>
                      </div>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <template v-if="pagedRuns.length > 0">
                    <template v-for="run in pagedRuns" :key="run.id">
                      <tr
                        class="border-t border-base-300"
                        :class="run.status === 'failure' ? 'bg-error/5' : ''"
                      >
                        <td class="text-sm">{{ formatDate(run.started_at) }}</td>
                        <td>
                          <span class="badge badge-sm" :class="statusBadge(run.status)">
                            {{ run.status }}
                          </span>
                        </td>
                        <td class="text-sm text-base-content/70">
                          {{ formatDuration(run.started_at, run.completed_at) }}
                        </td>
                        <td class="text-sm">{{ run.values_loaded_total ?? "—" }}</td>
                        <td class="text-sm text-success">{{ run.success_count ?? "—" }}</td>
                        <td class="text-sm text-error">{{ run.failure_count ?? "—" }}</td>
                        <td class="text-sm text-base-content/60">{{ run.skipped_count ?? "—" }}</td>
                      </tr>
                      <tr v-if="run.status === 'failure' && run.error_message">
                        <td
                          colspan="7"
                          class="bg-error/10 px-4 py-2 font-mono text-xs text-error border-t border-base-300"
                        >
                          {{ run.error_message }}
                        </td>
                      </tr>
                    </template>
                  </template>
                  <tr v-else>
                    <td colspan="7" class="text-center text-sm text-base-content/40 py-8">
                      No runs match your filters.
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <!-- Pagination -->
            <div class="flex items-center justify-between text-sm text-base-content/60">
              <span>
                <template v-if="filteredRuns.length === 0">0 runs</template>
                <template v-else>
                  {{ (currentPage - 1) * pageSize + 1 }}–{{ Math.min(currentPage * pageSize, filteredRuns.length) }}
                  of {{ filteredRuns.length }} runs
                </template>
              </span>
              <div class="join">
                <button
                  class="join-item btn btn-sm btn-ghost"
                  :disabled="currentPage === 1"
                  @click="currentPage--"
                >
                  «
                </button>
                <button
                  v-for="page in visiblePages"
                  :key="page"
                  class="join-item btn btn-sm"
                  :class="page === currentPage ? 'btn-primary' : 'btn-ghost'"
                  @click="currentPage = page"
                >
                  {{ page }}
                </button>
                <button
                  class="join-item btn btn-sm btn-ghost"
                  :disabled="currentPage === totalPages"
                  @click="currentPage++"
                >
                  »
                </button>
              </div>
            </div>

          </template>
        </div>
      </div>

    </template>

    <!-- Error -->
    <div v-if="error" class="mx-6 mt-4 alert alert-error text-sm">
      {{ error }}
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue"
import { useRouter, useRoute } from "vue-router"
import { api, type Task, type TaskRun, type HydroServerConnection, type Schedule } from "@/api"

const router = useRouter()
const route = useRoute()
const taskId = route.params.id as string

// ── State ─────────────────────────────────────────────────────────────────────
const task = ref<Task | null>(null)
const connection = ref<HydroServerConnection | null>(null)
const runs = ref<TaskRun[]>([])
const loading = ref(false)
const runsLoading = ref(false)
const running = ref(false)
const error = ref<string | null>(null)

// ── Tabs ──────────────────────────────────────────────────────────────────────
const tabs = [
  { key: "details", label: "Details" },
  { key: "mappings", label: "Datastream Mappings" },
  { key: "runs", label: "Run History" },
]
const activeTab = ref("details")

// ── Runs table state ──────────────────────────────────────────────────────────
const search = ref("")
const statusFilter = ref("")
const pageSize = ref<number>(10)
const currentPage = ref(1)
const sortKey = ref("started_at")
const sortDir = ref<"asc" | "desc">("desc")

const columns = [
  { key: "started_at", label: "Started" },
  { key: "status", label: "Status" },
  { key: "duration", label: "Duration" },
  { key: "values_loaded_total", label: "Loaded" },
  { key: "success_count", label: "Success" },
  { key: "failure_count", label: "Failure" },
  { key: "skipped_count", label: "Skipped" },
]

// ── Filtering + sorting + pagination ──────────────────────────────────────────
const filteredRuns = computed(() => {
  let result = [...runs.value]

  if (statusFilter.value) {
    result = result.filter(r => r.status === statusFilter.value)
  }

  if (search.value.trim()) {
    const q = search.value.toLowerCase()
    result = result.filter(r =>
      [
        r.status,
        r.started_at,
        r.completed_at,
        r.error_message,
        r.success_count?.toString(),
        r.failure_count?.toString(),
        r.skipped_count?.toString(),
        r.values_loaded_total?.toString(),
        r.earliest_timestamp,
        r.latest_timestamp,
      ]
        .filter(Boolean)
        .some(v => v!.toLowerCase().includes(q))
    )
  }

  result.sort((a, b) => {
    let aVal: string | number | null
    let bVal: string | number | null

    if (sortKey.value === "duration") {
      aVal = a.completed_at
        ? new Date(a.completed_at).getTime() - new Date(a.started_at).getTime()
        : -1
      bVal = b.completed_at
        ? new Date(b.completed_at).getTime() - new Date(b.started_at).getTime()
        : -1
    } else {
      aVal = (a as Record<string, unknown>)[sortKey.value] as string | number | null
      bVal = (b as Record<string, unknown>)[sortKey.value] as string | number | null
    }

    if (aVal == null) return 1
    if (bVal == null) return -1
    if (aVal < bVal) return sortDir.value === "asc" ? -1 : 1
    if (aVal > bVal) return sortDir.value === "asc" ? 1 : -1
    return 0
  })

  return result
})

const totalPages = computed(() => Math.max(1, Math.ceil(filteredRuns.value.length / pageSize.value)))

const pagedRuns = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value
  return filteredRuns.value.slice(start, start + pageSize.value)
})

const visiblePages = computed(() => {
  const total = totalPages.value
  const current = currentPage.value
  const delta = 2
  const pages: number[] = []
  for (let i = Math.max(1, current - delta); i <= Math.min(total, current + delta); i++) {
    pages.push(i)
  }
  return pages
})

function setSort(key: string) {
  if (sortKey.value === key) {
    sortDir.value = sortDir.value === "asc" ? "desc" : "asc"
  } else {
    sortKey.value = key
    sortDir.value = key === "started_at" ? "desc" : "asc"
  }
  currentPage.value = 1
}

// ── Load ──────────────────────────────────────────────────────────────────────
async function load() {
  loading.value = true
  error.value = null
  try {
    task.value = await api.tasks.get(taskId)
    if (task.value?.connection_id) {
      connection.value = await api.connections.get(task.value.connection_id)
    }
  } catch {
    error.value = "Failed to load task."
  } finally {
    loading.value = false
  }
}

async function loadRuns() {
  runsLoading.value = true
  try {
    runs.value = await api.runs.list(taskId)
  } catch {
    error.value = "Failed to load run history."
  } finally {
    runsLoading.value = false
  }
}

// ── Polling ───────────────────────────────────────────────────────────────────
let pollInterval: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await load()
  await loadRuns()
  pollInterval = setInterval(loadRuns, 5000)
})

onUnmounted(() => {
  if (pollInterval) clearInterval(pollInterval)
})

// ── Actions ───────────────────────────────────────────────────────────────────
async function runNow() {
  running.value = true
  try {
    await api.tasks.runNow(taskId)
    setTimeout(loadRuns, 1500)
  } catch {
    error.value = "Failed to trigger run."
  } finally {
    running.value = false
  }
}

async function toggleActive() {
  if (!task.value) return
  try {
    await api.tasks.update(taskId, {
      name: task.value.name,
      connection_id: task.value.connection_id,
      source_type: task.value.source_type,
      file_path: task.value.file_path,
      csv_delimiter: task.value.csv_delimiter,
      csv_header_row: task.value.csv_header_row,
      csv_timestamp_column: task.value.csv_timestamp_column,
      csv_timestamp_format: task.value.csv_timestamp_format,
      column_mappings: task.value.column_mappings,
      schedule: task.value.schedule,
      is_active: !task.value.is_active,
    })
    await load()
  } catch {
    error.value = "Failed to update task."
  }
}

function openEdit() {
  router.push({ name: "tasks", query: { edit: taskId } })
}

// ── Formatters ────────────────────────────────────────────────────────────────
function formatSchedule(schedule: Schedule | null | undefined): string {
  if (!schedule) return "No schedule"
  return `Every ${schedule.interval} ${schedule.period}`
}

function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—"
  return new Date(iso).toLocaleString()
}

function formatDuration(start: string, end: string | null): string {
  if (!end) return "—"
  const ms = new Date(end).getTime() - new Date(start).getTime()
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`
}

function statusBadge(status: string): string {
  return (
    ({
      success: "badge-success",
      failure: "badge-error",
      started: "badge-warning",
    } as Record<string, string>)[status] ?? "badge-ghost"
  )
}
</script>