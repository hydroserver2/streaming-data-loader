<script setup lang="ts">
import { computed, ref, watch } from 'vue'

import {
  getJobs,
  getJob,
  getJobLogs,
  getConfig,
  deleteJob,
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
const workspaceLabel = computed(
  () =>
    model.state.connectionSummary?.workspace_name?.trim() ||
    model.state.connectionSummary?.workspace_id?.trim() ||
    model.state.config?.server.workspace_id ||
    'Current workspace'
)
const datasourceCountLabel = computed(() =>
  jobs.value.length === 1 ? '1 source' : `${jobs.value.length} sources`
)
const jobStatusById = ref<Record<string, JobStatusSummary>>({})
const pendingDeleteJobId = ref<string | null>(null)
const deletingJobId = ref<string | null>(null)

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

function dashboardStatus(job: JobConfig): {
  label: 'Pending' | 'Up to Date' | 'Behind Schedule' | 'Needs Attention'
  badgeClass: string
} {
  const status = jobStatusById.value[job.id]?.status
  if (status === 'healthy' || status === 'running') {
    return { label: 'Up to Date', badgeClass: 'bg-emerald-900/60 text-emerald-300' }
  }

  if (status === 'warning') {
    return { label: 'Behind Schedule', badgeClass: 'bg-amber-900/60 text-amber-300' }
  }

  if (status === 'error' || status === 'disabled') {
    return { label: 'Needs Attention', badgeClass: 'bg-red-900/60 text-red-300' }
  }

  return { label: 'Pending', badgeClass: 'bg-sky-900/60 text-sky-300' }
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
          <h1 class="wizard-page-title">{{ workspaceLabel }}</h1>
          <p class="mapping-help">{{ datasourceCountLabel }}</p>
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
      <div class="mx-auto w-full max-w-7xl px-8">
        <FeedbackBanner :feedback="model.state.pipelineCreateFeedback" />
      </div>

      <div class="flex min-h-0 flex-col overflow-y-auto">
        <article
          v-for="(job, index) in jobs"
          :key="job.id"
          class="border-b border-white/6"
          :class="index % 2 === 0 ? 'bg-black/10' : 'bg-transparent'"
        >
          <div class="mx-auto flex max-w-7xl flex-col gap-3 px-8 py-4">
            <div class="flex items-start justify-between gap-3">
              <div class="mapping-source-stack min-w-0 flex-1">
                <p class="mapping-summary-title">{{ job.name }}</p>
              </div>
              <span
                class="inline-flex shrink-0 items-center rounded-full px-2.5 py-0.5 text-xs font-semibold"
                :class="dashboardStatus(job).badgeClass"
              >
                {{ dashboardStatus(job).label }}
              </span>
            </div>

            <div class="min-w-0">
              <p class="mapping-help break-all">{{ displayFileName(job.file_path) }}</p>
            </div>

            <div class="flex flex-wrap items-end justify-between gap-3">
              <div class="flex flex-wrap gap-2">
                <button
                  class="btn-ghost dashboard-item-button px-3 py-1.5 text-xs"
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
                  class="btn-ghost dashboard-item-button px-3 py-1.5 text-xs"
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
                  class="btn-ghost dashboard-item-button px-3 py-1.5 text-xs"
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
                  class="btn-ghost dashboard-item-button dashboard-item-button-logs px-3 py-1.5 text-xs"
                  type="button"
                  @click="void toggleLogs(job.id)"
                >
                  {{ isLogsOpen(job.id) ? 'Hide Logs' : 'View Logs' }}
                </button>
                <button
                  v-if="pendingDeleteJobId !== job.id"
                  class="btn-ghost dashboard-item-button dashboard-item-button-danger px-3 py-1.5 text-xs text-red-400 hover:text-red-300"
                  type="button"
                  :disabled="deletingJobId === job.id"
                  @click="requestDeleteJob(job.id)"
                >
                  {{ deletingJobId === job.id ? 'Deleting…' : 'Delete' }}
                </button>
                <template v-else>
                  <button
                    class="btn-ghost dashboard-item-button px-3 py-1.5 text-xs"
                    type="button"
                    @click="cancelDeleteJob(job.id)"
                  >
                    Cancel
                  </button>
                  <button
                    class="btn-ghost dashboard-item-button dashboard-item-button-danger px-3 py-1.5 text-xs text-red-400 hover:text-red-300"
                    type="button"
                    @click="void confirmDeleteJob(job.id)"
                  >
                    Confirm Delete
                  </button>
                </template>
              </div>

              <p class="mapping-help whitespace-nowrap text-right">
                {{ mappingCount(job) }}
                {{ mappingCount(job) === 1 ? 'mapping' : 'mappings' }}
              </p>
            </div>
          </div>

          <div
            v-if="isLogsOpen(job.id)"
            class="mt-4 rounded-2xl bg-[#0b0d0e] px-4 py-4"
          >
            <div v-if="diagnosticsLoading" class="mapping-help">
              Loading logs and status…
            </div>

            <div v-else-if="diagnosticsError" class="notice-error">
              {{ diagnosticsError }}
            </div>

            <div v-else-if="diagnosticsDetail" class="flex flex-col gap-4">
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="flex flex-col gap-2">
                  <div class="flex flex-wrap items-center gap-2">
                    <span
                      class="inline-flex items-center rounded-full px-3 py-1 text-xs font-medium"
                      :class="statusToneClass(diagnosticsDetail.status)"
                    >
                      {{ diagnosticsDetail.status }}
                    </span>
                    <span class="mapping-help">
                      {{ diagnosticsDetail.status_message }}
                    </span>
                  </div>
                  <div
                    v-if="diagnosticsDetail.last_error"
                    class="notice-error"
                  >
                    {{ diagnosticsDetail.last_error }}
                  </div>
                </div>

                <div class="flex flex-col items-end gap-1 text-right">
                  <p class="mapping-help">
                    Last run {{ formatTimestamp(diagnosticsDetail.last_run_at) }}
                  </p>
                  <p class="mapping-help">
                    Last push
                    {{ formatTimestamp(diagnosticsDetail.last_pushed_timestamp) }}
                  </p>
                </div>
              </div>

              <div class="flex flex-col gap-2">
                <p class="mapping-help uppercase tracking-[0.14em] text-slate-500">
                  Recent Logs
                </p>

                <div
                  v-if="displayedDiagnosticsLogs.length === 0"
                  class="mapping-help"
                >
                  No logs yet for this data source.
                </div>

                <div
                  v-else
                  class="max-h-72 overflow-auto rounded-xl bg-black/30 px-3 py-3"
                >
                  <div
                    v-for="entry in displayedDiagnosticsLogs"
                    :key="`${entry.timestamp}-${entry.level}-${entry.message}`"
                    class="grid gap-1 py-2 text-sm text-slate-300 first:pt-0 last:pb-0 md:grid-cols-[9rem_5rem_minmax(0,1fr)] md:gap-3"
                  >
                    <span class="font-mono text-xs text-slate-500">
                      {{ formatTimestamp(entry.timestamp) }}
                    </span>
                    <span
                      class="font-mono text-xs uppercase"
                      :class="
                        entry.level === 'error'
                          ? 'text-red-300'
                          : entry.level === 'warning'
                            ? 'text-amber-300'
                            : 'text-sky-300'
                      "
                    >
                      {{ entry.level }}
                    </span>
                    <span class="break-words">{{ entry.message }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </article>

        <article
          v-if="jobs.length === 0"
          class="mx-auto w-full max-w-7xl rounded-2xl bg-[#111315] px-5 py-6"
        >
          <p class="mapping-help">No data sources yet.</p>
        </article>
      </div>
    </div>
  </section>
</template>

<style scoped>
.dashboard-shell {
  width: 100%;
  max-width: none;
  height: 100vh;
  height: 100dvh;
  overflow: hidden;
  gap: 0;
  padding-top: calc(4.75rem - 2px);
  padding-bottom: 0;
  padding-left: 0;
  padding-right: 0;
}

.dashboard-item-button {
  border: 1px solid rgb(255 255 255 / 0.14);
  background: rgb(255 255 255 / 0.04);
  color: rgb(241 245 249 / 0.96);
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.02);
}

.dashboard-item-button:hover:not(:disabled) {
  border-color: rgb(255 255 255 / 0.24);
  background: rgb(255 255 255 / 0.08);
}

.dashboard-item-button:disabled {
  opacity: 0.55;
}

.dashboard-item-button-danger {
  border-color: rgb(248 113 113 / 0.3);
  background: rgb(127 29 29 / 0.18);
}

.dashboard-item-button-danger:hover:not(:disabled) {
  border-color: rgb(248 113 113 / 0.42);
  background: rgb(127 29 29 / 0.28);
}

.dashboard-item-button-logs {
  border-color: rgb(56 189 248 / 0.32);
  background: rgb(8 47 73 / 0.32);
  color: rgb(186 230 253 / 0.98);
}

.dashboard-item-button-logs:hover:not(:disabled) {
  border-color: rgb(56 189 248 / 0.5);
  background: rgb(12 74 110 / 0.42);
}
</style>
