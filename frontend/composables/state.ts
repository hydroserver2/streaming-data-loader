import { reactive } from "vue"

import { createAuthFieldStates, type AuthFieldName, type Feedback, type FieldValidationState } from "../auth-submit"
import { getRouteFromHash, type AppRoute } from "../router"
import type {
  AppConfig,
  ConnectionState,
  ConnectionTestResponse,
  CsvPreviewResponse,
  DatastreamSummary,
  HealthResponse,
  JobSummary,
  ServerConfig,
} from "../api"

// ── Types ─────────────────────────────────────────────────────────────────
export type PipelineMappingDraft = {
  csvColumn: string
  datastreamId: string
}

export type PipelineFormState = {
  name: string
  filePath: string
  scheduleMinutes: number
  hasHeaderRow: boolean
  headerRow: number
  dataStartRow: number
  delimiter: string
  timestampColumn: string
  timestampFormat: string
  timezone: string
  mappings: PipelineMappingDraft[]
}

export type PreviewSelectionTarget =
  | "header-row"
  | "data-start-row"
  | "timestamp-column"
  | null

export type PreviewRowSelectionTarget = Exclude<PreviewSelectionTarget, "timestamp-column" | null>

export type WizardStep = "file-config" | "column-mapping"

type UiState = {
  route: AppRoute
  health: HealthResponse | null
  config: AppConfig | null
  jobs: JobSummary[]
  datastreams: DatastreamSummary[]
  connectionSummary: ConnectionTestResponse | null
  loading: boolean
  bootstrapError: string | null
  settingsFeedback: Feedback
  welcomeFeedback: Feedback
  pipelineFeedback: Feedback
  lastConnectionState: ConnectionState | null
  settingsEditMode: boolean
  wizardStep: WizardStep
  pipelineForm: PipelineFormState
  pipelinePreview: CsvPreviewResponse | null
  pipelineErrors: string[]
  datastreamsError: string | null
  authDraft: ServerConfig
  authFieldStates: Record<AuthFieldName, FieldValidationState>
  authSubmitting: boolean
  lastAuthValidationServer: ServerConfig | null
  lastAuthValidationResult: ConnectionTestResponse | null
  pipelineSelectionTarget: PreviewSelectionTarget
  pipelinePreviewRowsRequested: number
}

// ── Constants ──────────────────────────────────────────────────────────────
export const PREVIEW_PAGE_SIZE = 50
export const APP_NAME = "HydroServer Streaming Data Loader"
export const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key"

// ── Factories ──────────────────────────────────────────────────────────────
export function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
    workspace_id: "",
  }
}

export function createEmptyPipelineForm(): PipelineFormState {
  return {
    name: "",
    filePath: "",
    scheduleMinutes: 15,
    hasHeaderRow: true,
    headerRow: 3,
    dataStartRow: 4,
    delimiter: ",",
    timestampColumn: "Timestamp",
    timestampFormat: "%Y-%m-%d %H:%M:%S",
    timezone: "America/Denver",
    mappings: [],
  }
}

// ── Singleton reactive state ───────────────────────────────────────────────
export const state = reactive<UiState>({
  route: getRouteFromHash(),
  health: null,
  config: null,
  jobs: [],
  datastreams: [],
  connectionSummary: null,
  loading: true,
  bootstrapError: null,
  settingsFeedback: null,
  welcomeFeedback: null,
  pipelineFeedback: null,
  lastConnectionState: null,
  settingsEditMode: false,
  wizardStep: "file-config",
  pipelineForm: createEmptyPipelineForm(),
  pipelinePreview: null,
  pipelineErrors: [],
  datastreamsError: null,
  authDraft: emptyServerConfig(),
  authFieldStates: createAuthFieldStates(),
  authSubmitting: false,
  lastAuthValidationServer: null,
  lastAuthValidationResult: null,
  pipelineSelectionTarget: null,
  pipelinePreviewRowsRequested: PREVIEW_PAGE_SIZE,
})
