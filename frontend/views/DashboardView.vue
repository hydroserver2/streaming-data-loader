<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import {
  getJobs,
  getJob,
  getJobLogs,
  getConfig,
  deleteJob,
  revealFileInFolder,
  type JobConfig,
  type JobDetail,
  type JobLogEntry,
  type JobStatus,
  type JobStatusSummary,
} from '../api'
import AccountMenuButton from '../components/AccountMenuButton.vue'
import FeedbackBanner from '../components/FeedbackBanner.vue'
import { useAppModel } from '../composables/useAppModel'
import { navigate } from '../router'

const model = useAppModel()

const jobs = computed(() => model.state.config?.jobs ?? [])
type DashboardStatusLabel =
  | 'Pending'
  | 'Up to Date'
  | 'Behind Schedule'
  | 'Needs Attention'

type DashboardStatusTone = 'info' | 'healthy' | 'warning' | 'danger'

type DashboardStatusMeta = {
  label: DashboardStatusLabel
  tone: DashboardStatusTone
}

const DASHBOARD_STATUS_ORDER: DashboardStatusLabel[] = [
  'Pending',
  'Up to Date',
  'Behind Schedule',
  'Needs Attention',
]

const STATUS_TONE_CLASS: Record<DashboardStatusTone, string> = {
  healthy: 'data-source-status-healthy',
  warning: 'data-source-status-warning',
  danger: 'data-source-status-danger',
  info: 'data-source-status-info',
}

const workspaceLabel = computed(
  () =>
    model.state.connectionSummary?.workspace_name?.trim() ||
    model.state.connectionSummary?.workspace_id?.trim() ||
    model.state.config?.server.workspace_id ||
    'Current workspace'
)
const datasourceCount = computed(() => jobs.value.length)
const jobStatusById = ref<Record<string, JobStatusSummary>>({})
const pendingDeleteJobId = ref<string | null>(null)
const deletingJobId = ref<string | null>(null)
const textFilter = ref('')
const statusFilter = ref<'all' | DashboardStatusLabel>('all')

type NavSection = 'file' | 'setup' | 'mappings'
const pendingNavigation = ref<{ jobId: string; section: NavSection } | null>(null)

async function navigateTo(jobId: string, section: NavSection): Promise<void> {
  if (pendingNavigation.value) return
  pendingNavigation.value = { jobId, section }
  try {
    if (section === 'file') await model.editPipelineSourceFile(jobId)
    else if (section === 'setup') await model.editPipelineCsvSetup(jobId)
    else await model.editPipelineMappings(jobId)
  } finally {
    pendingNavigation.value = null
  }
}

function requestDeleteJob(jobId: string): void {
  if (pendingDeleteJobId.value === jobId) {
    return
  }
  pendingDeleteJobId.value = jobId
}

function cancelDeleteJob(jobId: string): void {
  if (pendingDeleteJobId.value === jobId) {
    pendingDeleteJobId.value = null
  }
}

async function confirmDeleteJob(jobId: string): Promise<void> {
  if (pendingDeleteJobId.value !== jobId) return
  deletingJobId.value = jobId
  pendingDeleteJobId.value = null
  try {
    await deleteJob(jobId)
    model.state.config = await getConfig()
  } finally {
    deletingJobId.value = null
  }
}
const displayedDiagnosticsLogs = computed(() => [...diagnosticsLogs.value].reverse())
const diagnosticsJobId = ref<string | null>(null)
const diagnosticsLoading = ref(false)
const diagnosticsError = ref<string | null>(null)
const diagnosticsDetail = ref<JobDetail | null>(null)
const diagnosticsLogs = ref<JobLogEntry[]>([])

function mappingCount(job: JobConfig): number {
  return job.column_mappings.length
}

function dashboardStatusFor(status: JobStatus | undefined): DashboardStatusMeta {
  if (status === 'healthy' || status === 'running') {
    return { label: 'Up to Date', tone: 'healthy' }
  }

  if (status === 'warning') {
    return { label: 'Behind Schedule', tone: 'warning' }
  }

  if (status === 'error' || status === 'disabled') {
    return { label: 'Needs Attention', tone: 'danger' }
  }

  return { label: 'Pending', tone: 'info' }
}

function dashboardStatus(job: JobConfig): DashboardStatusMeta {
  return dashboardStatusFor(jobStatusById.value[job.id]?.status)
}

function statusClass(meta: DashboardStatusMeta): string {
  return STATUS_TONE_CLASS[meta.tone]
}

function isLogsOpen(jobId: string): boolean {
  return diagnosticsJobId.value === jobId
}

function statusToneClass(status: JobDetail['status']): string {
  if (status === 'healthy') return 'pill-success'
  if (status === 'warning') return 'pill-warning'
  if (status === 'error') return 'pill-danger'
  if (status === 'disabled') return 'pill-muted'
  return 'pill-info'
}

function formatTimestamp(value: string | null): string {
  if (!value) return 'Never'

  const timestamp = new Date(value)
  if (Number.isNaN(timestamp.getTime())) return value

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(timestamp)
}

function displayFileName(filePath: string): string {
  const segments = filePath.split(/[\\/]/)
  return segments[segments.length - 1] || filePath
}

const statusCounts = computed(() => {
  const counts = Object.fromEntries(
    DASHBOARD_STATUS_ORDER.map((label) => [label, 0])
  ) as Record<DashboardStatusLabel, number>

  for (const job of jobs.value) {
    counts[dashboardStatus(job).label] += 1
  }

  return counts
})

const filteredJobs = computed(() => {
  const query = textFilter.value.trim().toLowerCase()

  return jobs.value.filter((job) => {
    const statusMatches =
      statusFilter.value === 'all' || dashboardStatus(job).label === statusFilter.value

    if (!statusMatches) return false
    if (!query) return true

    const name = job.name.toLowerCase()
    const fileName = displayFileName(job.file_path).toLowerCase()

    return name.includes(query) || fileName.includes(query)
  })
})

function isDesktopRuntime(): boolean {
  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in (window as Window & typeof globalThis)
  )
}

async function openFileLocation(filePath: string): Promise<void> {
  try {
    await revealFileInFolder(filePath)
  } catch (error) {
    console.error('Failed to open file location.', error)
  }
}

async function toggleLogs(jobId: string): Promise<void> {
  if (diagnosticsJobId.value === jobId) {
    diagnosticsJobId.value = null
    diagnosticsDetail.value = null
    diagnosticsLogs.value = []
    diagnosticsError.value = null
    diagnosticsLoading.value = false
    return
  }

  diagnosticsJobId.value = jobId
  diagnosticsLoading.value = true
  diagnosticsError.value = null
  diagnosticsDetail.value = null
  diagnosticsLogs.value = []

  try {
    const [detail, logs] = await Promise.all([getJob(jobId), getJobLogs(jobId)])
    if (diagnosticsJobId.value !== jobId) return
    diagnosticsDetail.value = detail
    diagnosticsLogs.value = logs
  } catch (error) {
    if (diagnosticsJobId.value !== jobId) return
    diagnosticsError.value =
      error instanceof Error
        ? error.message
        : "Couldn't load logs for this data source."
  } finally {
    if (diagnosticsJobId.value === jobId) {
      diagnosticsLoading.value = false
    }
  }
}

async function loadJobStatuses(): Promise<void> {
  if (jobs.value.length === 0) {
    jobStatusById.value = {}
    return
  }

  try {
    const summaries = await getJobs()
    const nextStatuses = Object.fromEntries(
      summaries.map((summary) => [summary.id, summary])
    ) as Record<string, JobStatusSummary>
    jobStatusById.value = nextStatuses
  } catch {
    jobStatusById.value = {}
  }
}

watch(
  () => jobs.value.map((job) => job.id).join('|'),
  () => {
    void loadJobStatuses()
  },
  { immediate: true }
)
</script>

<template>
  <section
    class="page-shell animate-fade-in onboarding-shell pipeline-editor-shell dashboard-shell"
  >
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <h1 class="wizard-page-title">Data Source Dashboard</h1>
          <p class="mapping-help">{{ workspaceLabel }}</p>
        </div>
        <div class="button-row wizard-actions">
          <button
            class="btn-primary wizard-nav-button"
            type="button"
            @click="navigate('jobs-new')"
          >
            + Add Data Source
          </button>
          <AccountMenuButton />
        </div>
      </div>
    </header>

    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div class="mx-auto flex w-full max-w-7xl flex-col gap-4 px-8 pt-4">
        <FeedbackBanner :feedback="model.state.pipelineCreateFeedback" />
      </div>

      <section class="dashboard-toolbar">
        <label class="dashboard-filter-field">
          <input
            v-model="textFilter"
            class="input dashboard-filter-input"
            type="text"
            placeholder="Filter by name or file"
          />
        </label>

        <div class="dashboard-status-filters">
          <button
            class="dashboard-status-filter"
            :class="{ 'dashboard-status-filter-active': statusFilter === 'all' }"
            type="button"
            @click="statusFilter = 'all'"
          >
            <span>All</span>
            <span class="dashboard-status-filter-count">{{ datasourceCount }}</span>
          </button>
          <button
            v-for="label in DASHBOARD_STATUS_ORDER"
            :key="label"
            class="dashboard-status-filter"
            :class="{ 'dashboard-status-filter-active': statusFilter === label }"
            type="button"
            @click="statusFilter = label"
          >
            <span>{{ label }}</span>
            <span class="dashboard-status-filter-count">{{ statusCounts[label] }}</span>
          </button>
        </div>
      </section>

      <div class="dashboard-body">
        <div class="dashboard-list">
          <article
            v-for="job in filteredJobs"
            :key="job.id"
            class="data-source-row"
          >
            <div class="data-source-row-head">
              <div class="data-source-row-title-block">
                <p class="data-source-row-title">{{ job.name }}</p>
                <button
                  v-if="isDesktopRuntime()"
                  class="data-source-row-file data-source-row-file-link"
                  type="button"
                  @click="void openFileLocation(job.file_path)"
                >
                  {{ displayFileName(job.file_path) }}
                </button>
                <p v-else class="data-source-row-file">
                  {{ displayFileName(job.file_path) }}
                </p>
              </div>
              <span
                class="data-source-status"
                :class="statusClass(dashboardStatus(job))"
              >
                {{ dashboardStatus(job).label }}
              </span>
            </div>

            <div class="data-source-row-meta">
              <div class="data-source-row-actions">
                <button
                  class="data-source-action"
                  type="button"
                  :disabled="pendingNavigation !== null"
                  @click="void navigateTo(job.id, 'file')"
                >
                  {{
                    pendingNavigation?.jobId === job.id && pendingNavigation?.section === 'file'
                      ? 'Loading…'
                      : 'Source File'
                  }}
                </button>
                <button
                  class="data-source-action"
                  type="button"
                  :disabled="pendingNavigation !== null"
                  @click="void navigateTo(job.id, 'setup')"
                >
                  {{
                    pendingNavigation?.jobId === job.id && pendingNavigation?.section === 'setup'
                      ? 'Loading…'
                      : 'CSV Setup'
                  }}
                </button>
                <button
                  class="data-source-action"
                  type="button"
                  :disabled="pendingNavigation !== null"
                  @click="void navigateTo(job.id, 'mappings')"
                >
                  {{
                    pendingNavigation?.jobId === job.id && pendingNavigation?.section === 'mappings'
                      ? 'Loading…'
                      : 'Mappings'
                  }}
                </button>
                <button
                  class="data-source-action data-source-action-logs"
                  type="button"
                  @click="void toggleLogs(job.id)"
                >
                  {{ isLogsOpen(job.id) ? 'Hide Logs' : 'View Logs' }}
                </button>
                <button
                  v-if="pendingDeleteJobId !== job.id"
                  class="data-source-action data-source-action-danger"
                  type="button"
                  :disabled="deletingJobId === job.id"
                  @click="requestDeleteJob(job.id)"
                >
                  {{ deletingJobId === job.id ? 'Deleting…' : 'Delete' }}
                </button>
                <template v-else>
                  <button
                    class="data-source-action"
                    type="button"
                    @click="cancelDeleteJob(job.id)"
                  >
                    Cancel
                  </button>
                  <button
                    class="data-source-action data-source-action-danger"
                    type="button"
                    @click="void confirmDeleteJob(job.id)"
                  >
                    Confirm Delete
                  </button>
                </template>
              </div>

              <p class="data-source-row-mapping-count">
                {{ mappingCount(job) }}
                {{ mappingCount(job) === 1 ? 'mapping' : 'mappings' }}
              </p>
            </div>

            <div v-if="isLogsOpen(job.id)" class="data-source-logs-panel">
              <div v-if="diagnosticsLoading" class="mapping-help">
                Loading logs and status…
              </div>

              <div v-else-if="diagnosticsError" class="notice-error">
                {{ diagnosticsError }}
              </div>

              <div v-else-if="diagnosticsDetail" class="flex flex-col gap-4">
                <div class="data-source-logs-header">
                  <div class="flex flex-col gap-2">
                    <div class="data-source-logs-status-row">
                      <span :class="statusToneClass(diagnosticsDetail.status)">
                        {{ diagnosticsDetail.status }}
                      </span>
                      <span class="mapping-help">
                        {{ diagnosticsDetail.status_message }}
                      </span>
                    </div>
                    <div v-if="diagnosticsDetail.last_error" class="notice-error">
                      {{ diagnosticsDetail.last_error }}
                    </div>
                  </div>

                  <div class="data-source-logs-times">
                    <p>
                      Last run {{ formatTimestamp(diagnosticsDetail.last_run_at) }}
                    </p>
                    <p>
                      Last push
                      {{ formatTimestamp(diagnosticsDetail.last_pushed_timestamp) }}
                    </p>
                  </div>
                </div>

                <div class="flex flex-col gap-2">
                  <p class="data-source-logs-section-label">Recent Logs</p>

                  <div
                    v-if="displayedDiagnosticsLogs.length === 0"
                    class="mapping-help"
                  >
                    No logs yet for this data source.
                  </div>

                  <div v-else class="data-source-logs-list">
                    <div
                      v-for="entry in displayedDiagnosticsLogs"
                      :key="`${entry.timestamp}-${entry.level}-${entry.message}`"
                      class="data-source-log-entry"
                    >
                      <span class="data-source-log-timestamp">
                        {{ formatTimestamp(entry.timestamp) }}
                      </span>
                      <span
                        class="data-source-log-level"
                        :class="
                          entry.level === 'error'
                            ? 'data-source-log-level-error'
                            : entry.level === 'warning'
                              ? 'data-source-log-level-warning'
                              : 'data-source-log-level-info'
                        "
                      >
                        {{ entry.level }}
                      </span>
                      <span class="wrap-break-word">{{ entry.message }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </article>

          <p v-if="jobs.length === 0" class="data-source-empty">
            No data sources yet.
          </p>
          <p
            v-else-if="filteredJobs.length === 0"
            class="data-source-empty"
          >
            No data sources match the current filters.
          </p>
        </div>
      </div>
    </div>
  </section>
</template>
