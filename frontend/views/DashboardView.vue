<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'

import {
  getJobs,
  getJobLogs,
  getConfig,
  deleteJob,
  revealFileInFolder,
  runJobNow,
  updateJob,
  type JobConfig,
  type JobLogEntry,
  type JobStatus,
  type JobStatusSummary,
} from '../api'
import AccountMenuButton from '../components/AccountMenuButton.vue'
import { useAppModel } from '../composables/useAppModel'
import { navigate } from '../router'

const model = useAppModel()

const jobs = computed(() => model.state.config?.jobs ?? [])
type DashboardStatusLabel =
  | 'Running'
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
  'Running',
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
type RunButtonState = 'idle' | 'requested'
const runButtonStateById = ref<Record<string, RunButtonState>>({})
const editingNameJobId = ref<string | null>(null)
const editingNameValue = ref('')
const savingNameJobId = ref<string | null>(null)
const editingNameInput = ref<HTMLInputElement | null>(null)
const textFilter = ref('')
const statusFilter = ref<'all' | DashboardStatusLabel>('all')

type NavSection = 'file' | 'setup' | 'mappings'
const pendingNavigation = ref<{ jobId: string; section: NavSection } | null>(null)
const RUN_BUTTON_MIN_LOCK_MS = 1000
const RUN_REFRESH_OFFSETS_MS = [0, 800, 2000, 4000] as const

function wait(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, milliseconds)
  })
}

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

async function handleRunNow(jobId: string): Promise<void> {
  if (isRunButtonDisabled(jobId)) return
  const requestedAt = Date.now()
  setRunButtonState(jobId, 'requested')
  setJobStatusOverride(jobId, {
    status: 'running',
    status_message: 'Run requested.',
    last_error: null,
  })

  try {
    await runJobNow(jobId)
    void refreshJobAfterRun(jobId)
  } catch (error) {
    console.error("Couldn't start this data source right now.", error)
    await loadJobStatuses()
  } finally {
    const elapsed = Date.now() - requestedAt
    if (elapsed < RUN_BUTTON_MIN_LOCK_MS) {
      await wait(RUN_BUTTON_MIN_LOCK_MS - elapsed)
    }
    clearRunButtonState(jobId)
  }
}

async function beginEditingName(job: JobConfig): Promise<void> {
  if (savingNameJobId.value) return

  editingNameJobId.value = job.id
  editingNameValue.value = job.name

  await nextTick()
  editingNameInput.value?.focus()
  editingNameInput.value?.select()
}

function cancelEditingName(): void {
  if (savingNameJobId.value) return
  editingNameJobId.value = null
  editingNameValue.value = ''
}

async function saveEditingName(job: JobConfig): Promise<void> {
  if (savingNameJobId.value === job.id) return

  const name = editingNameValue.value.trim()
  if (!name) {
    await nextTick()
    editingNameInput.value?.focus()
    return
  }

  if (name === job.name) {
    cancelEditingName()
    return
  }

  savingNameJobId.value = job.id

  try {
    await updateJob(job.id, {
      name,
      enabled: job.enabled,
      file_path: job.file_path,
      schedule_minutes: job.schedule_minutes,
      file_config: job.file_config,
      column_mappings: job.column_mappings,
    })
    model.state.config = await getConfig()
    editingNameJobId.value = null
    editingNameValue.value = ''
  } catch (error) {
    console.error("Couldn't update the data source name right now.", error)
  } finally {
    savingNameJobId.value = null
  }
}

function isEditingName(jobId: string): boolean {
  return editingNameJobId.value === jobId
}

function isSavingName(jobId: string): boolean {
  return savingNameJobId.value === jobId
}
const diagnosticsJobId = ref<string | null>(null)
const diagnosticsLoading = ref(false)
const diagnosticsError = ref<string | null>(null)
const diagnosticsLogs = ref<JobLogEntry[]>([])
const showAllDiagnosticsLogs = ref(false)
const displayedDiagnosticsLogs = computed(() => [...diagnosticsLogs.value].reverse())
const visibleDiagnosticsLogs = computed(() =>
  showAllDiagnosticsLogs.value
    ? displayedDiagnosticsLogs.value
    : displayedDiagnosticsLogs.value.slice(0, 5)
)
const hasAdditionalDiagnosticsLogs = computed(
  () => displayedDiagnosticsLogs.value.length > visibleDiagnosticsLogs.value.length
)

function mappingCount(job: JobConfig): number {
  return job.column_mappings.length
}

function dashboardStatusFor(status: JobStatus | undefined): DashboardStatusMeta {
  if (status === 'running') {
    return { label: 'Running', tone: 'info' }
  }

  if (status === 'healthy') {
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

function runButtonState(jobId: string): RunButtonState {
  return runButtonStateById.value[jobId] ?? 'idle'
}

function setRunButtonState(jobId: string, state: RunButtonState): void {
  runButtonStateById.value[jobId] = state
}

function clearRunButtonState(jobId: string): void {
  delete runButtonStateById.value[jobId]
}

function isJobRunning(jobId: string): boolean {
  return jobStatusById.value[jobId]?.status === 'running'
}

function isRunButtonDisabled(jobId: string): boolean {
  return runButtonState(jobId) !== 'idle' || isJobRunning(jobId)
}

function isRunButtonActive(jobId: string): boolean {
  return isRunButtonDisabled(jobId)
}

function runButtonLabel(jobId: string): string {
  const state = runButtonState(jobId)
  if (state === 'requested') return 'Run requested…'
  if (isJobRunning(jobId)) return 'Running…'
  return 'Run Now'
}

function setJobStatusOverride(
  jobId: string,
  overrides: Partial<JobStatusSummary>
): void {
  const existing = jobStatusById.value[jobId]
  const job = jobs.value.find((entry) => entry.id === jobId)
  if (!existing && !job) return

  const fileConfig = existing?.file_config ?? job?.file_config
  if (!fileConfig) return

  jobStatusById.value = {
    ...jobStatusById.value,
    [jobId]: {
      id: existing?.id ?? jobId,
      name: existing?.name ?? job?.name ?? '',
      enabled: existing?.enabled ?? job?.enabled ?? true,
      file_path: existing?.file_path ?? job?.file_path ?? '',
      schedule_minutes: existing?.schedule_minutes ?? job?.schedule_minutes ?? 0,
      file_config: fileConfig,
      column_mappings: existing?.column_mappings ?? job?.column_mappings ?? [],
      status: existing?.status ?? 'pending',
      status_message: existing?.status_message ?? '',
      last_pushed_timestamp: existing?.last_pushed_timestamp ?? null,
      last_run_at: existing?.last_run_at ?? null,
      last_error: existing?.last_error ?? null,
      ...overrides,
    },
  }
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
    diagnosticsLogs.value = []
    diagnosticsError.value = null
    diagnosticsLoading.value = false
    showAllDiagnosticsLogs.value = false
    return
  }

  diagnosticsJobId.value = jobId
  diagnosticsLoading.value = true
  diagnosticsError.value = null
  diagnosticsLogs.value = []
  showAllDiagnosticsLogs.value = false

  try {
    const logs = await getJobLogs(jobId)
    if (diagnosticsJobId.value !== jobId) return
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

async function refreshOpenLogs(jobId: string): Promise<void> {
  if (diagnosticsJobId.value !== jobId) return

  try {
    const logs = await getJobLogs(jobId)
    if (diagnosticsJobId.value !== jobId) return
    diagnosticsLogs.value = logs
    diagnosticsError.value = null
  } catch (error) {
    if (diagnosticsJobId.value !== jobId) return
    diagnosticsError.value =
      error instanceof Error
        ? error.message
        : "Couldn't load logs for this data source."
  }
}

async function refreshJobAfterRun(jobId: string): Promise<void> {
  let elapsed = 0

  for (const offset of RUN_REFRESH_OFFSETS_MS) {
    const delay = offset - elapsed
    if (delay > 0) {
      await wait(delay)
    }
    elapsed = offset

    await loadJobStatuses()
    await refreshOpenLogs(jobId)
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
                <div v-if="isEditingName(job.id)" class="data-source-row-title-edit">
                  <input
                    ref="editingNameInput"
                    v-model="editingNameValue"
                    class="input data-source-name-input"
                    type="text"
                    :disabled="isSavingName(job.id)"
                    @keydown.enter.prevent="void saveEditingName(job)"
                    @keydown.esc.prevent="cancelEditingName()"
                  />
                  <div class="data-source-row-title-edit-actions">
                    <button
                      class="data-source-action"
                      type="button"
                      :disabled="isSavingName(job.id)"
                      @click="cancelEditingName()"
                    >
                      Cancel
                    </button>
                    <button
                      class="data-source-action data-source-action-save"
                      type="button"
                      :disabled="isSavingName(job.id)"
                      @click="void saveEditingName(job)"
                    >
                      {{ isSavingName(job.id) ? 'Saving…' : 'Save' }}
                    </button>
                  </div>
                </div>
                <div v-else class="data-source-row-title-line">
                  <p class="data-source-row-title">{{ job.name }}</p>
                  <button
                    class="data-source-name-edit-trigger"
                    type="button"
                    aria-label="Edit data source name"
                    @click="void beginEditingName(job)"
                  >
                    <svg viewBox="0 0 20 20" fill="none" aria-hidden="true">
                      <path
                        d="M13.75 3.75a1.768 1.768 0 0 1 2.5 2.5l-8.5 8.5-3 0.5 0.5-3 8.5-8.5Z"
                        stroke="currentColor"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="1.5"
                      />
                    </svg>
                  </button>
                </div>
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
              <div class="data-source-row-head-actions">
                <button
                  class="data-source-action data-source-action-run"
                  :class="{ 'data-source-action-run-active': isRunButtonActive(job.id) }"
                  type="button"
                  :disabled="isRunButtonDisabled(job.id)"
                  @click="void handleRunNow(job.id)"
                >
                  {{ runButtonLabel(job.id) }}
                </button>
                <span
                  class="data-source-status"
                  :class="statusClass(dashboardStatus(job))"
                >
                  {{ dashboardStatus(job).label }}
                </span>
              </div>
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
                Loading logs…
              </div>

              <div v-else-if="diagnosticsError" class="notice-error">
                {{ diagnosticsError }}
              </div>

              <div v-else class="flex flex-col gap-2">
                <div>
                  <div
                    v-if="displayedDiagnosticsLogs.length === 0"
                    class="mapping-help"
                  >
                    No logs yet for this data source.
                  </div>

                  <div v-else class="data-source-logs-list">
                    <div
                      v-for="entry in visibleDiagnosticsLogs"
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

                  <button
                    v-if="hasAdditionalDiagnosticsLogs"
                    class="data-source-action mt-2"
                    type="button"
                    @click="showAllDiagnosticsLogs = true"
                  >
                    View more
                  </button>
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
