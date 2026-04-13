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

export { APP_NAME, API_KEY_DOCS_URL, PREVIEW_PAGE_INCREMENT, PREVIEW_PAGE_SIZE }
export type { PreviewSelectionTarget, PreviewRowSelectionTarget } from "./state"

export * from "./useAuth"
export * from "./usePipeline"

const isConnected = computed(
  () => state.connectionSummary?.ok === true && state.lastConnectionState === "connected"
)

function syncRouteState(): void {
  let route = getRouteFromHash()

  if (!state.loading && !state.bootstrapError) {
    if (!isConnected.value) {
      if (route !== "welcome") {
        navigate("welcome")
        route = "welcome"
      }
    } else {
      if (route === "jobs-new-mapping" && !state.pipelineReadyForMapping) {
        navigate("jobs-new")
        route = "jobs-new"
      } else if (route !== "jobs-new" && route !== "jobs-new-mapping") {
        navigate("jobs-new")
        route = "jobs-new"
      }
    }
  }

  state.route = route
}

watch(
  [isConnected, () => state.loading, () => state.pipelineReadyForMapping],
  syncRouteState
)

export const useWelcomeSurface = computed(
  () =>
    Boolean(
      state.loading ||
        state.bootstrapError ||
        state.route === "welcome" ||
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
  state.bootstrapError = null
  state.welcomeFeedback = null
  state.settingsFeedback = null
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
  } catch (error) {
    state.bootstrapError =
      error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`
  } finally {
    state.loading = false
    syncRouteState()
  }
}

export function init(): void {
  window.addEventListener("hashchange", () => {
    state.settingsFeedback = null
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
  buildPipelineTransformerSettings,
  createPipelineDatasource,
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
  useWelcomeSurface,
  parsedPreviewRows,
  previewHeaders,
  selectedPreviewTimestampColumn,
  buildPipelineTransformerSettings,
  buildPipelineColumnMappings,
  submitPipelineConfig,
  createPipelineDatasource,
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
