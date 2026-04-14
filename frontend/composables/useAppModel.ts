import { computed, watch } from "vue"

import { getConfig, getHealth } from "../api"
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
import type { AppRoute } from "../router"

export { APP_NAME, API_KEY_DOCS_URL, PREVIEW_PAGE_INCREMENT, PREVIEW_PAGE_SIZE }
export type { PreviewSelectionTarget, PreviewRowSelectionTarget } from "./state"

export * from "./useAuth"
export * from "./usePipeline"

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
}): AppRoute {
  const { route, hasSavedDatasources, pipelineReadyForMapping } = params
  const fallbackRoute: AppRoute = hasSavedDatasources ? "dashboard" : "jobs-new"

  if (route === "jobs-new-mapping" && !pipelineReadyForMapping) {
    return fallbackRoute
  }

  if (route === "welcome") {
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

function syncRouteState(): void {
  let route = getRouteFromHash()

  if (!state.loading) {
    if (!isConnected.value) {
      if (route !== "welcome") {
        navigate("welcome")
        route = "welcome"
      }
    } else {
      const nextRoute = resolveAuthenticatedRoute({
        route,
        hasSavedDatasources: hasSavedDatasources.value,
        pipelineReadyForMapping: state.pipelineReadyForMapping,
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
  ],
  syncRouteState
)

export const useWelcomeSurface = computed(
  () =>
    Boolean(
      state.loading ||
        state.route === "welcome" ||
        state.route === "dashboard" ||
        state.route === "jobs-new" ||
        state.route === "jobs-new-mapping"
    )
)

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
      const [health, config] = await Promise.all([getHealth(), getConfig()])
      return { health, config }
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
  syncRouteState()

  try {
    const { health, config } = await loadInitialState()
    state.health = health
    state.config = config
    state.authDraft = { ...emptyServerConfig(), ...config.server }
    state.lastConnectionState = health.connection.state

    if (serverConfigured(config.server)) {
      await syncAuthenticationStatus(config.server)
    }
  } catch {
    // bootstrap failed; routing will fall back to the welcome/connection screen
  } finally {
    state.loading = false
    syncRouteState()
  }
}

export function init(): void {
  window.addEventListener("hashchange", () => {
    syncRouteState()
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
} as const

export function useAppModel() {
  return model
}
