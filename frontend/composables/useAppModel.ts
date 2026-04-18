import { computed, watch } from "vue"

import {
  getBootstrap,
  subscribeToDaemonStatus,
  type DaemonStatusSnapshot,
} from "../api/hydroserver"
import { getServiceStatus } from "../api/os-service"
import { getRouteFromHash, navigate } from "../router"
import {
  state,
  emptyServerConfig,
  APP_NAME,
  API_KEY_DOCS_URL,
  PREVIEW_PAGE_INCREMENT,
  PREVIEW_PAGE_SIZE,
} from "./state"
import { syncAuthenticationStatus, serverConfigured } from "./useAuth"
import {
  installBackgroundService,
  isServiceReady,
  refreshServiceStatus,
  restartBackgroundService,
  uninstallBackgroundService,
} from "./useService"
import { isTauriRuntime, normalizeError } from "../api/runtime"
import type { AppRoute } from "../router"

export { APP_NAME, API_KEY_DOCS_URL, PREVIEW_PAGE_INCREMENT, PREVIEW_PAGE_SIZE }
export type { PreviewSelectionTarget, PreviewRowSelectionTarget } from "./state"

export * from "./useAuth"
export * from "./usePipeline"
export * from "./useService"

const isConnected = computed(
  () => state.connectionSummary?.ok === true && state.lastConnectionState === "connected"
)

const hasSavedDatasources = computed(
  () => (state.config?.jobs?.length ?? 0) > 0
)

export function resolveAuthenticatedRoute(params: {
  route: AppRoute
  hasSavedDatasources: boolean
  pipelineReadyForMapping: boolean
  serviceReady: boolean
}): AppRoute {
  const { route, hasSavedDatasources, pipelineReadyForMapping, serviceReady } = params
  const fallbackRoute: AppRoute = hasSavedDatasources ? "dashboard" : "jobs-new"

  if (!serviceReady) {
    return "service"
  }

  if (route === "jobs-new-mapping" && !pipelineReadyForMapping) {
    return fallbackRoute
  }

  if (route === "welcome" || route === "service") {
    return fallbackRoute
  }

  if (route === "dashboard" && !hasSavedDatasources) {
    return "jobs-new"
  }

  if (
    route !== "dashboard" &&
    route !== "jobs-new" &&
    route !== "jobs-new-mapping"
  ) {
    return fallbackRoute
  }

  return route
}

export function requiresDesktopServiceSetup(params: {
  tauriRuntime: boolean
  serviceReady: boolean
  daemonReady: boolean
}): boolean {
  const { tauriRuntime, serviceReady, daemonReady } = params
  return tauriRuntime && (!serviceReady || !daemonReady)
}

export function shouldHydrateAuthDraftFromDaemon(params: {
  authSubmitting: boolean
  authDraftDirty: boolean
}): boolean {
  const { authSubmitting, authDraftDirty } = params
  return !authSubmitting && !authDraftDirty
}

function bootstrapServiceErrorMessage(params: {
  error: unknown
  serviceStatusInstalled: boolean
  serviceStatusRunning: boolean
  serviceSupported: boolean
}): string {
  const { error, serviceStatusInstalled, serviceStatusRunning, serviceSupported } = params

  if (!serviceSupported) {
    return normalizeError(error).message
  }

  if (!serviceStatusInstalled) {
    return "Install the background service to continue."
  }

  if (!serviceStatusRunning) {
    return "Restart the background service to continue."
  }

  return "Couldn't connect to the local background service. Restart it to continue."
}

function syncRouteState(): void {
  let route = getRouteFromHash()

  if (!state.loading) {
    if (
      requiresDesktopServiceSetup({
        tauriRuntime: isTauriRuntime(),
        serviceReady: isServiceReady(state.serviceStatus),
        daemonReady: state.health !== null && state.config !== null,
      })
    ) {
      if (route !== "service") {
        navigate("service")
        route = "service"
      }
    } else if (!isConnected.value) {
      if (route !== "welcome") {
        navigate("welcome")
        route = "welcome"
      }
    } else {
      const nextRoute = resolveAuthenticatedRoute({
        route,
        hasSavedDatasources: hasSavedDatasources.value,
        pipelineReadyForMapping: state.pipelineReadyForMapping,
        serviceReady: isServiceReady(state.serviceStatus),
      })
      if (nextRoute !== route) {
        navigate(nextRoute)
        route = nextRoute
      }
    }
  }

  state.route = route
}

watch(
  [
    isConnected,
    hasSavedDatasources,
    () => state.loading,
    () => state.pipelineReadyForMapping,
    () => state.serviceStatus?.installed,
    () => state.serviceStatus?.running,
  ],
  syncRouteState
)

export const useWelcomeSurface = computed(
  () =>
    Boolean(
      state.loading ||
        state.route === "welcome" ||
        state.route === "service" ||
        state.route === "dashboard" ||
        state.route === "jobs-new" ||
        state.route === "jobs-new-mapping"
    )
)

let stopStatusSubscription: (() => void) | null = null

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
      const [bootstrapResponse, serviceStatus] = await Promise.all([
        getBootstrap(),
        getServiceStatus(),
      ])
      return { ...bootstrapResponse, serviceStatus }
    } catch (error) {
      lastError = error
      if (attempt === STARTUP_RETRY_ATTEMPTS || !isTransientError(error)) throw error
      await sleep(STARTUP_RETRY_DELAY_MS)
    }
  }
  throw lastError instanceof Error ? lastError : new Error(`Failed to load ${APP_NAME}.`)
}

function applyDaemonStatusSnapshot(snapshot: DaemonStatusSnapshot): void {
  state.health = snapshot.health
  state.config = snapshot.config
  state.jobStatuses = snapshot.jobs

  if (
    shouldHydrateAuthDraftFromDaemon({
      authSubmitting: state.authSubmitting,
      authDraftDirty: state.authDraftDirty,
    })
  ) {
    state.authDraft = { ...emptyServerConfig(), ...snapshot.config.server }
    state.authDraftDirty = false
  }

  if (snapshot.health.connection.state === "not_configured") {
    state.connectionSummary = null
    state.lastConnectionState = "not_configured"
  } else if (!state.lastConnectionState || state.lastConnectionState === "not_configured") {
    state.lastConnectionState = snapshot.health.connection.state
  }
}

function ensureStatusSubscription(): void {
  stopStatusSubscription?.()
  stopStatusSubscription = subscribeToDaemonStatus({
    onStatus(snapshot) {
      applyDaemonStatusSnapshot(snapshot)
    },
    onError(error) {
      console.error("The daemon status stream disconnected.", error)
    },
  })
}

export async function bootstrap(): Promise<void> {
  state.loading = true
  syncRouteState()

  try {
    ensureStatusSubscription()
    const { health, config, jobs, serviceStatus } = await loadInitialState()
    applyDaemonStatusSnapshot({ health, config, jobs })
    state.serviceStatus = serviceStatus
    state.serviceActionError = null
    state.lastConnectionState = health.connection.state

    if (serverConfigured(config.server)) {
      await syncAuthenticationStatus(config.server)
    }
  } catch (error) {
    if (isTauriRuntime()) {
      const serviceStatus = await refreshServiceStatus()
      state.serviceActionError = bootstrapServiceErrorMessage({
        error,
        serviceStatusInstalled: Boolean(serviceStatus?.installed),
        serviceStatusRunning: Boolean(serviceStatus?.running),
        serviceSupported: serviceStatus?.supported !== false,
      })
    }
  } finally {
    state.loading = false
    syncRouteState()
  }
}

export function init(): void {
  window.addEventListener("hashchange", () => {
    syncRouteState()
  })
  window.addEventListener("focus", () => {
    if (!state.loading && isConnected.value) {
      void refreshServiceStatus()
    }
  })

  syncRouteState()
  void bootstrap()
}

import {
  updateAuthDraftField,
  toggleAuthMode,
  submitAuthConfig,
  disconnectHydroServer,
} from "./useAuth"

import {
  abandonPipelineCreation,
  buildPipelineTransformerSettings,
  createPipelineDatasource,
  editPipelineCsvSetup,
  editPipelineMappings,
  editPipelineSourceFile,
  submitPipelineConfig,
  parsedPreviewRows,
  previewHeaders,
  selectedPreviewTimestampColumn,
  canShowMorePreviewLines,
  updatePipelineField,
  setPipelineHasHeaderRow,
  setPipelineIdentifierType,
  applyPreviewLineSelection,
  applyPreviewColumnSelection,
  updateHeaderRowFromPreview,
  updateDataStartRowFromPreview,
  loadPipelinePreview,
  showMorePreviewLines,
  browseForCsvPath,
} from "./usePipeline"

import {
  buildPipelineColumnMappings,
  pipelineDatastreamBrowserEntries,
  clearPipelineMapping,
  datastreamOptionsForThing,
  loadPipelineDatastreams,
  pipelineMappingRows,
  pipelineMappingSourceColumns,
  pipelineThingOptions,
  syncPipelineMappingDrafts,
  updatePipelineMappingDatastream,
  updatePipelineMappingThing,
} from "./useMapping"

const model = {
  state,
  APP_NAME,
  API_KEY_DOCS_URL,
  PREVIEW_PAGE_INCREMENT,
  PREVIEW_PAGE_SIZE,
  isConnected,
  hasSavedDatasources,
  useWelcomeSurface,
  parsedPreviewRows,
  previewHeaders,
  selectedPreviewTimestampColumn,
  abandonPipelineCreation,
  buildPipelineTransformerSettings,
  buildPipelineColumnMappings,
  submitPipelineConfig,
  createPipelineDatasource,
  editPipelineCsvSetup,
  editPipelineMappings,
  editPipelineSourceFile,
  canShowMorePreviewLines,
  pipelineDatastreamBrowserEntries,
  pipelineMappingRows,
  pipelineMappingSourceColumns,
  pipelineThingOptions,
  init,
  bootstrap,
  updateAuthDraftField,
  toggleAuthMode,
  submitAuthConfig,
  disconnectHydroServer,
  updatePipelineField,
  setPipelineHasHeaderRow,
  setPipelineIdentifierType,
  applyPreviewLineSelection,
  applyPreviewColumnSelection,
  updateHeaderRowFromPreview,
  updateDataStartRowFromPreview,
  loadPipelinePreview,
  loadPipelineDatastreams,
  syncPipelineMappingDrafts,
  datastreamOptionsForThing,
  updatePipelineMappingThing,
  updatePipelineMappingDatastream,
  clearPipelineMapping,
  showMorePreviewLines,
  browseForCsvPath,
  refreshServiceStatus,
  installBackgroundService,
  restartBackgroundService,
  uninstallBackgroundService,
} as const

export function useAppModel() {
  return model
}
