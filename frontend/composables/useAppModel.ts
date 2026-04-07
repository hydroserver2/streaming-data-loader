import { computed, watch } from "vue"

import { getConfig, getHealth, listJobs } from "../api"
import { getRouteFromHash, navigate, type AppRoute } from "../router"
import { state, emptyServerConfig, APP_NAME, API_KEY_DOCS_URL, PREVIEW_PAGE_SIZE } from "./state"
import { syncAuthenticationStatus, loadDatastreams, serverConfigured } from "./useAuth"
import { refreshJobs } from "./useJobs"

export { APP_NAME, API_KEY_DOCS_URL, PREVIEW_PAGE_SIZE }
export type { PreviewSelectionTarget, PreviewRowSelectionTarget, WizardStep } from "./state"

// ── Re-export everything views need in one import ─────────────────────────
export * from "./useAuth"
export * from "./usePipeline"
export * from "./useJobs"

// ── Routing ────────────────────────────────────────────────────────────────
function serverConfiguredFromState(): boolean {
  return serverConfigured(state.config?.server)
}

const isConnected = computed(
  () => state.connectionSummary?.ok === true && state.lastConnectionState === "connected"
)

function syncRouteState(): void {
  let route = getRouteFromHash()

  if (!state.loading && !state.bootstrapError) {
    if (!isConnected.value && route !== "settings" && route !== "welcome") {
      navigate("welcome")
      route = "welcome"
    } else if (
      isConnected.value &&
      state.jobs.length === 0 &&
      (route === "dashboard" || route === "welcome")
    ) {
      navigate("jobs-new")
      route = "jobs-new"
    }
  }

  state.route = route
}

// Replace scattered manual syncRouteState() calls: watch the two conditions
// that drive routing and sync automatically.
watch([isConnected, () => state.jobs.length, () => state.loading], syncRouteState)

// ── Shell computed ─────────────────────────────────────────────────────────
function onboardingRoute(route: AppRoute): boolean {
  return route === "welcome" || (route === "jobs-new" && state.jobs.length === 0)
}

export const showSidebar = computed(
  () => !state.loading && !onboardingRoute(state.route) && !state.bootstrapError
)

export const useWelcomeSurface = computed(
  () => Boolean(state.loading || state.bootstrapError || onboardingRoute(state.route))
)

export function connectionIndicator(): { label: string; className: string } {
  if (!serverConfiguredFromState()) {
    return { label: "HydroServer not configured", className: "status-dot bg-slate-300" }
  }
  if (isConnected.value) {
    return { label: "Connected to HydroServer", className: "status-dot bg-emerald-500" }
  }
  if (state.lastConnectionState === "error") {
    return { label: "HydroServer authentication error", className: "status-dot bg-rose-500" }
  }
  return { label: "HydroServer configured", className: "status-dot bg-sky-500" }
}

// ── Bootstrap ──────────────────────────────────────────────────────────────
const STARTUP_RETRY_ATTEMPTS = 12
const STARTUP_RETRY_DELAY_MS = 350

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function isTransientError(error: unknown): boolean {
  if (!(error instanceof Error)) return false
  const msg = error.message.toLowerCase()
  return (
    msg.includes("failed to fetch") ||
    msg.includes("networkerror") ||
    msg.includes("status 500") ||
    msg.includes("status 502") ||
    msg.includes("status 503") ||
    msg.includes("status 504")
  )
}

async function loadInitialState() {
  let lastError: unknown = null
  for (let attempt = 1; attempt <= STARTUP_RETRY_ATTEMPTS; attempt++) {
    try {
      const [health, config, jobs] = await Promise.all([getHealth(), getConfig(), listJobs()])
      return { health, config, jobs }
    } catch (error) {
      lastError = error
      if (attempt === STARTUP_RETRY_ATTEMPTS || !isTransientError(error)) throw error
      await sleep(STARTUP_RETRY_DELAY_MS)
    }
  }
  throw lastError instanceof Error ? lastError : new Error(`Failed to load ${APP_NAME}.`)
}

export async function bootstrap(): Promise<void> {
  state.loading = true
  state.bootstrapError = null
  state.welcomeFeedback = null
  state.settingsFeedback = null
  syncRouteState()

  try {
    const { health, config, jobs } = await loadInitialState()
    state.health = health
    state.config = config
    state.authDraft = { ...emptyServerConfig(), ...config.server }
    state.jobs = jobs
    state.lastConnectionState = health.connection.state

    if (serverConfigured(config.server)) {
      const result = await syncAuthenticationStatus(config.server)
      if (result.ok) await loadDatastreams()
    }
  } catch (error) {
    state.bootstrapError =
      error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`
  } finally {
    state.loading = false
    syncRouteState()
  }
}

// ── Init (called once from App.vue onMounted) ──────────────────────────────
export function init(): void {
  window.addEventListener("hashchange", () => {
    state.settingsFeedback = null
    syncRouteState()
  })

  window.setInterval(() => void refreshJobs(), 30_000)

  syncRouteState()
  void bootstrap()
}

// ── Singleton model ────────────────────────────────────────────────────────
import {
  updateAuthDraftField,
  toggleAuthMode,
  submitAuthConfig,
  disconnectHydroServer,
  changeCredentials,
  cancelCredentialEdit,
} from "./useAuth"

import {
  parsedPreviewRows,
  previewHeaders,
  canShowMorePreviewLines,
  updatePipelineField,
  updateMapping,
  setPipelineHasHeaderRow,
  applyPreviewLineSelection,
  applyPreviewColumnSelection,
  updateHeaderRowFromPreview,
  updateDataStartRowFromPreview,
  loadPipelinePreview,
  showMorePreviewLines,
  browseForCsvPath,
  submitPipeline,
  advanceToMapping,
  backToFileConfig,
} from "./usePipeline"

import {
  handleRunJob,
  handleToggleJob,
  handleDeleteJob,
} from "./useJobs"

const model = {
  state,
  APP_NAME,
  API_KEY_DOCS_URL,
  PREVIEW_PAGE_SIZE,
  isConnected,
  showSidebar,
  useWelcomeSurface,
  parsedPreviewRows,
  previewHeaders,
  canShowMorePreviewLines,
  connectionIndicator,
  init,
  bootstrap,
  updateAuthDraftField,
  toggleAuthMode,
  submitAuthConfig,
  disconnectHydroServer,
  changeCredentials,
  cancelCredentialEdit,
  updatePipelineField,
  updateMapping,
  setPipelineHasHeaderRow,
  applyPreviewLineSelection,
  applyPreviewColumnSelection,
  updateHeaderRowFromPreview,
  updateDataStartRowFromPreview,
  loadPipelinePreview,
  showMorePreviewLines,
  browseForCsvPath,
  submitPipeline,
  advanceToMapping,
  backToFileConfig,
  handleRunJob,
  handleToggleJob,
  handleDeleteJob,
} as const

export function useAppModel() {
  return model
}
