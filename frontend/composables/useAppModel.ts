import { computed, reactive } from "vue"

import {
  clearServerConfig,
  createJob,
  deleteJob,
  disableJob,
  enableJob,
  getConfig,
  getCsvPreview,
  getDatastreams,
  getHealth,
  listJobs,
  runJob,
  testConnection,
  updateServerConfig,
  validateServerUrl,
  type AppConfig,
  type AuthType,
  type ConnectionState,
  type ConnectionTestResponse,
  type CsvPreviewResponse,
  type DatastreamSummary,
  type HealthResponse,
  type JobSummary,
  type ServerConfig,
} from "../api"
import {
  applyConnectionValidationResult,
  createAuthFieldStates,
  fieldFormFeedbackTarget,
  resetAuthFieldStates,
  runAuthSubmission,
  validateAuthFieldsForSubmit,
  type AuthFieldName,
  type Feedback,
  type FieldValidationState,
} from "../auth-submit"
import { getRouteFromHash, navigate, type AppRoute } from "../router"

export const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key"
export const APP_NAME = "HydroServer Streaming Data Loader"
export const PREVIEW_PAGE_SIZE = 50

const STARTUP_RETRY_ATTEMPTS = 12
const STARTUP_RETRY_DELAY_MS = 350

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

export type PreviewRowSelectionTarget = Exclude<
  PreviewSelectionTarget,
  "timestamp-column" | null
>

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

function createEmptyPipelineForm(): PipelineFormState {
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

function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
    workspace_id: "",
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function isTransientBootstrapError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false
  }

  const message = error.message.toLowerCase()
  return (
    message.includes("failed to fetch") ||
    message.includes("networkerror") ||
    message.includes("status 500") ||
    message.includes("status 502") ||
    message.includes("status 503") ||
    message.includes("status 504")
  )
}

function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean)
  return segments.at(-1) ?? path
}

function parseDelimitedLine(line: string, delimiter: string): string[] {
  if (!delimiter) {
    return [line]
  }

  const cells: string[] = []
  let current = ""
  let inQuotes = false

  for (let index = 0; index < line.length; index += 1) {
    const character = line[index]

    if (character === '"') {
      if (inQuotes && line[index + 1] === '"') {
        current += '"'
        index += 1
      } else {
        inQuotes = !inQuotes
      }
      continue
    }

    if (!inQuotes && line.startsWith(delimiter, index)) {
      cells.push(current)
      current = ""
      index += delimiter.length - 1
      continue
    }

    current += character
  }

  cells.push(current)
  return cells
}

function normalizePreviewHeaderName(value: string, index: number): string {
  const cleaned = value.trim()
  return cleaned || `Column ${index + 1}`
}

const state = reactive<UiState>({
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

const parsedPreviewRows = computed(() => {
  if (!state.pipelinePreview) {
    return []
  }

  return state.pipelinePreview.raw_lines.map((line) =>
    parseDelimitedLine(line, state.pipelineForm.delimiter)
  )
})

const previewHeaders = computed(() => {
  const rows = parsedPreviewRows.value
  const columnCount = rows.reduce((max, row) => Math.max(max, row.length), 0)

  if (!state.pipelineForm.hasHeaderRow) {
    const dataRows = rows.slice(Math.max(state.pipelineForm.dataStartRow - 1, 0))
    const dataColumnCount = (dataRows.length > 0 ? dataRows : rows).reduce(
      (max, row) => Math.max(max, row.length),
      0
    )
    return Array.from(
      { length: dataColumnCount },
      (_, index) => `Column ${index + 1}`
    )
  }

  const headerRow = rows[state.pipelineForm.headerRow - 1] ?? []
  return Array.from({ length: columnCount }, (_, index) =>
    normalizePreviewHeaderName(headerRow[index] ?? "", index)
  )
})

const isConnected = computed(
  () =>
    state.connectionSummary?.ok === true &&
    state.lastConnectionState === "connected"
)

function onboardingRoute(route: AppRoute): boolean {
  return route === "welcome" || (route === "jobs-new" && state.jobs.length === 0)
}

const showSidebar = computed(
  () => !state.loading && !onboardingRoute(state.route) && !state.bootstrapError
)

const useWelcomeSurface = computed(
  () => Boolean(state.loading || state.bootstrapError || onboardingRoute(state.route))
)

function connectionIndicator(): { label: string; className: string } {
  if (!serverConfigured(state.config?.server)) {
    return {
      label: "HydroServer not configured",
      className: "status-dot bg-slate-300",
    }
  }

  if (isConnected.value) {
    return {
      label: "Connected to HydroServer",
      className: "status-dot bg-emerald-500",
    }
  }

  if (state.lastConnectionState === "error") {
    return {
      label: "HydroServer authentication error",
      className: "status-dot bg-rose-500",
    }
  }

  return {
    label: "HydroServer configured",
    className: "status-dot bg-sky-500",
  }
}

function serverConfigured(server: ServerConfig | null | undefined): boolean {
  if (!server?.url.trim()) {
    return false
  }

  if (server.auth_type === "userpass") {
    return Boolean(server.username.trim() && server.password.trim())
  }

  return Boolean(server.api_key.trim())
}

function resetStateAuthFieldStates(authType: AuthType): void {
  resetAuthFieldStates(state.authFieldStates, authType)
}

function markField(
  field: AuthFieldName,
  nextState: FieldValidationState["state"],
  message: string | null = null
): void {
  state.authFieldStates[field] = { state: nextState, message }
}

function clearAuthFormFeedback(formId: "welcome-form" | "settings-form"): void {
  state[fieldFormFeedbackTarget(formId)] = null
}

function clearAuthValidationCache(): void {
  state.lastAuthValidationServer = null
  state.lastAuthValidationResult = null
}

function setServerDraft(server: ServerConfig): void {
  state.authDraft = { ...server }
}

function normalizeServerDraft(): ServerConfig {
  const server = state.authDraft
  return {
    auth_type: server.auth_type,
    url: server.url.trim(),
    api_key:
      server.auth_type === "apikey" ? server.api_key.trim() : server.api_key,
    username:
      server.auth_type === "userpass" ? server.username.trim() : server.username,
    password:
      server.auth_type === "userpass" ? server.password.trim() : server.password,
    workspace_id: "",
  }
}

function updateAuthDraftField(
  formId: "welcome-form" | "settings-form",
  field: AuthFieldName,
  value: string
): void {
  state.authDraft[field] = value
  clearAuthFormFeedback(formId)
  clearAuthValidationCache()
  markField(field, "idle")
}

function toggleAuthMode(formId: "welcome-form" | "settings-form"): void {
  const nextAuthType: AuthType =
    state.authDraft.auth_type === "apikey" ? "userpass" : "apikey"
  setServerDraft({
    ...state.authDraft,
    auth_type: nextAuthType,
  })
  resetStateAuthFieldStates(nextAuthType)
  clearAuthFormFeedback(formId)
  clearAuthValidationCache()
}

function activePreviewRowTarget(): PreviewRowSelectionTarget | null {
  return state.pipelineSelectionTarget === "header-row" ||
    state.pipelineSelectionTarget === "data-start-row"
    ? state.pipelineSelectionTarget
    : null
}

function previewHandleLine(target: PreviewRowSelectionTarget): number | null {
  if (target === "header-row") {
    return state.pipelineForm.hasHeaderRow ? state.pipelineForm.headerRow : null
  }

  return state.pipelineForm.dataStartRow
}

function activeTimestampColumn(): string {
  return state.pipelineForm.timestampColumn
}

function previewColumnClass(columnName: string): string {
  if (columnName === state.pipelineForm.timestampColumn) {
    return "preview-col-timestamp"
  }

  const mapped = state.pipelineForm.mappings.find(
    (mapping) => mapping.csvColumn === columnName && mapping.datastreamId
  )
  return mapped ? "preview-col-mapped" : ""
}

function previewFieldClass(target: Exclude<PreviewSelectionTarget, null>): string {
  const active =
    target === "timestamp-column"
      ? state.pipelineSelectionTarget === target
      : activePreviewRowTarget() === target

  const toneClass =
    target === "header-row"
      ? "preview-bound-field-header"
      : target === "data-start-row"
      ? "preview-bound-field-data"
      : "preview-bound-field-timestamp"

  return active
    ? `field preview-bound-field preview-bound-field-active ${toneClass}`
    : "field preview-bound-field"
}

function previewGuidanceText(): string {
  const activeTarget = activePreviewRowTarget()

  if (activeTarget === "header-row") {
    return "Drag the HEADER handle, or click a row to place it."
  }

  if (activeTarget === "data-start-row") {
    return "Drag the DATA START handle, or click the first data row."
  }

  if (state.pipelineSelectionTarget === "timestamp-column") {
    return "Drag the TIMESTAMP handle, or click a column header to place it."
  }

  return state.pipelineForm.hasHeaderRow
    ? "Drag the HEADER, DATA START, and TIMESTAMP handles, or click a row or column to place them."
    : "Drag the DATA START and TIMESTAMP handles, or click a row or column to place them."
}

function pipelineMappingsByColumn(): Map<string, string> {
  return new Map(
    state.pipelineForm.mappings.map((mapping) => [
      mapping.csvColumn,
      mapping.datastreamId,
    ])
  )
}

function initializeMappings(headers: string[]): void {
  const existing = pipelineMappingsByColumn()
  state.pipelineForm.mappings = headers
    .filter((header) => header !== state.pipelineForm.timestampColumn)
    .map((header) => ({
      csvColumn: header,
      datastreamId: existing.get(header) ?? "",
    }))
}

function syncPipelineSelectionsWithPreview(): void {
  const headers = previewHeaders.value

  if (headers.length === 0) {
    state.pipelineForm.mappings = []
    return
  }

  const preferredTimestamp =
    headers.find((header) => header.toLowerCase().includes("time")) ?? headers[0]

  state.pipelineForm.timestampColumn = headers.includes(
    state.pipelineForm.timestampColumn
  )
    ? state.pipelineForm.timestampColumn
    : preferredTimestamp

  initializeMappings(headers)
}

function applyPreview(path: string, preview: CsvPreviewResponse): void {
  state.pipelinePreview = preview
  state.pipelineForm.filePath = path
  state.pipelineForm.hasHeaderRow = preview.detected_header_row !== null
  state.pipelineForm.headerRow =
    preview.detected_header_row ?? state.pipelineForm.headerRow
  state.pipelineForm.dataStartRow =
    preview.detected_data_start_row ?? state.pipelineForm.dataStartRow
  state.pipelineForm.delimiter =
    preview.detected_delimiter || state.pipelineForm.delimiter
  state.pipelineSelectionTarget = null

  if (!state.pipelineForm.name.trim()) {
    const inferred = basename(path).replace(/\.[^.]+$/, "")
    state.pipelineForm.name = inferred
  }

  syncPipelineSelectionsWithPreview()
}

function updateHeaderRowFromPreview(lineNumber: number): void {
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = lineNumber
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1
  }
  syncPipelineSelectionsWithPreview()
}

function updateDataStartRowFromPreview(lineNumber: number): void {
  state.pipelineForm.dataStartRow = Math.max(
    state.pipelineForm.hasHeaderRow ? 2 : 1,
    lineNumber
  )
  if (
    state.pipelineForm.hasHeaderRow &&
    state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow
  ) {
    state.pipelineForm.headerRow = state.pipelineForm.dataStartRow - 1
  }
  syncPipelineSelectionsWithPreview()
}

function setPipelineHasHeaderRow(enabled: boolean): void {
  state.pipelineForm.hasHeaderRow = enabled

  if (!enabled && state.pipelineSelectionTarget === "header-row") {
    state.pipelineSelectionTarget = null
  }

  if (enabled && state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow) {
    state.pipelineForm.headerRow = Math.max(1, state.pipelineForm.dataStartRow - 1)
  }

  syncPipelineSelectionsWithPreview()
}

function applyPreviewLineSelection(lineNumber: number): void {
  if (state.pipelineSelectionTarget === "header-row") {
    updateHeaderRowFromPreview(lineNumber)
    state.pipelineSelectionTarget = null
    return
  }

  if (state.pipelineSelectionTarget === "data-start-row") {
    updateDataStartRowFromPreview(lineNumber)
    state.pipelineSelectionTarget = null
  }
}

function applyPreviewColumnSelection(columnName: string): void {
  if (
    state.pipelineSelectionTarget &&
    state.pipelineSelectionTarget !== "timestamp-column"
  ) {
    return
  }

  state.pipelineForm.timestampColumn = columnName
  initializeMappings(previewHeaders.value)
  state.pipelineSelectionTarget = null
}

function canShowMorePreviewLines(): boolean {
  if (!state.pipelinePreview) {
    return false
  }

  return state.pipelinePreview.raw_lines.length < state.pipelinePreview.total_lines
}

function updatePipelineField(name: string, value: string): void {
  state.pipelineFeedback = null
  state.pipelineErrors = []

  switch (name) {
    case "pipeline_name":
      state.pipelineForm.name = value
      break
    case "file_path":
      state.pipelineForm.filePath = value
      state.pipelinePreview = null
      state.pipelineSelectionTarget = null
      state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
      break
    case "schedule_minutes":
      state.pipelineForm.scheduleMinutes = Number(value) || 15
      break
    case "header_row":
      state.pipelineForm.headerRow = Number(value) || 1
      syncPipelineSelectionsWithPreview()
      break
    case "data_start_row":
      state.pipelineForm.dataStartRow = Number(value) || 1
      syncPipelineSelectionsWithPreview()
      break
    case "delimiter":
      state.pipelineForm.delimiter = value || ","
      syncPipelineSelectionsWithPreview()
      break
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value
      initializeMappings(previewHeaders.value)
      break
    case "timestamp_format":
      state.pipelineForm.timestampFormat = value
      break
    case "timezone":
      state.pipelineForm.timezone = value
      break
    default:
      break
  }
}

function updateMapping(csvColumn: string, datastreamId: string): void {
  state.pipelineFeedback = null
  state.pipelineErrors = []
  const mapping = state.pipelineForm.mappings.find(
    (item) => item.csvColumn === csvColumn
  )
  if (mapping) {
    mapping.datastreamId = datastreamId
  }
}

function validatePipeline(): string[] {
  const errors: string[] = []
  const headers = previewHeaders.value
  const selectedMappings = state.pipelineForm.mappings.filter(
    (mapping) => mapping.datastreamId
  )
  const datastreamIds = new Set(state.datastreams.map((datastream) => datastream.id))
  const seenTargets = new Set<string>()

  if (!isConnected.value) {
    errors.push("Connect to HydroServer before saving a pipeline.")
  }

  if (!state.pipelineForm.name.trim()) {
    errors.push("Give the pipeline a name.")
  }

  if (!state.pipelineForm.filePath.trim()) {
    errors.push(`Choose the CSV file ${APP_NAME} should watch.`)
  }

  if (!state.pipelinePreview) {
    errors.push("Load a CSV preview before saving the pipeline.")
  }

  if (state.pipelineForm.hasHeaderRow && state.pipelineForm.headerRow < 1) {
    errors.push("Header row must be 1 or greater.")
  }

  if (
    state.pipelineForm.hasHeaderRow &&
    state.pipelineForm.dataStartRow <= state.pipelineForm.headerRow
  ) {
    errors.push("Data start row must come after the header row.")
  }

  if (!state.pipelineForm.hasHeaderRow && state.pipelineForm.dataStartRow < 1) {
    errors.push("Data start row must be 1 or greater.")
  }

  if (headers.length > 0 && !headers.includes(state.pipelineForm.timestampColumn)) {
    errors.push("Choose a timestamp column that exists in the previewed CSV header.")
  }

  if (selectedMappings.length === 0) {
    errors.push("Map at least one source column to a HydroServer datastream.")
  }

  for (const mapping of selectedMappings) {
    if (!datastreamIds.has(mapping.datastreamId)) {
      errors.push(
        `The selected target for ${mapping.csvColumn} is not a valid HydroServer datastream.`
      )
    }

    if (seenTargets.has(mapping.datastreamId)) {
      errors.push("Each target datastream can only be mapped once in this flow.")
    }

    seenTargets.add(mapping.datastreamId)
  }

  return errors
}

async function loadInitialStateWithRetry(): Promise<{
  health: HealthResponse
  config: AppConfig
  jobs: JobSummary[]
}> {
  let lastError: unknown = null

  for (let attempt = 1; attempt <= STARTUP_RETRY_ATTEMPTS; attempt += 1) {
    try {
      const [health, config, jobs] = await Promise.all([
        getHealth(),
        getConfig(),
        listJobs(),
      ])
      return { health, config, jobs }
    } catch (error) {
      lastError = error

      if (attempt === STARTUP_RETRY_ATTEMPTS || !isTransientBootstrapError(error)) {
        throw error
      }

      await sleep(STARTUP_RETRY_DELAY_MS)
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`Failed to load ${APP_NAME}.`)
}

async function syncAuthenticationStatus(
  server: ServerConfig
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server)
  state.lastAuthValidationServer = server
  state.lastAuthValidationResult = result
  state.connectionSummary = result
  state.lastConnectionState = result.state

  if (result.ok && result.workspace_id) {
    if (state.config) {
      state.config.server.workspace_id = result.workspace_id
    }
    state.authDraft.workspace_id = result.workspace_id
  }

  if (!result.ok) {
    state.datastreams = []
    state.datastreamsError = null
  }

  return result
}

async function loadDatastreams(): Promise<void> {
  try {
    state.datastreams = await getDatastreams()
    state.datastreamsError = null
  } catch (error) {
    state.datastreams = []
    state.datastreamsError =
      error instanceof Error ? error.message : "Couldn't load HydroServer datastreams."
  }
}

function syncRouteState(): void {
  let currentRoute = getRouteFromHash()

  if (!state.loading && !state.bootstrapError) {
    if (!isConnected.value && currentRoute !== "settings" && currentRoute !== "welcome") {
      navigate("welcome")
      currentRoute = "welcome"
    } else if (
      isConnected.value &&
      state.jobs.length === 0 &&
      (currentRoute === "dashboard" || currentRoute === "welcome")
    ) {
      navigate("jobs-new")
      currentRoute = "jobs-new"
    }
  }

  state.route = currentRoute
}

async function bootstrap(): Promise<void> {
  state.loading = true
  state.bootstrapError = null
  state.welcomeFeedback = null
  state.settingsFeedback = null
  syncRouteState()

  try {
    const { health, config, jobs } = await loadInitialStateWithRetry()
    state.health = health
    state.config = config
    state.authDraft = {
      ...emptyServerConfig(),
      ...config.server,
    }
    state.jobs = jobs
    state.lastConnectionState = health.connection.state

    if (serverConfigured(config.server)) {
      const result = await syncAuthenticationStatus(config.server)
      if (result.ok) {
        await loadDatastreams()
      }
    }
  } catch (error) {
    state.bootstrapError =
      error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`
  } finally {
    state.loading = false
    syncRouteState()
  }
}

async function refreshJobs(): Promise<void> {
  if (state.bootstrapError || state.loading) {
    return
  }

  try {
    state.jobs = await listJobs()
    syncRouteState()
  } catch {
    // Keep existing UI state on polling failure.
  }
}

async function loadPipelinePreview(
  path: string,
  rows = PREVIEW_PAGE_SIZE
): Promise<void> {
  if (!path.trim()) {
    state.pipelineFeedback = {
      tone: "error",
      message: "Enter or choose a CSV file path first.",
    }
    return
  }

  try {
    const preview = await getCsvPreview(path.trim(), rows)
    applyPreview(path.trim(), preview)
    state.pipelinePreviewRowsRequested = rows
    state.pipelineErrors = []
    state.pipelineFeedback = null
  } catch (error) {
    state.pipelinePreview = null
    state.pipelineSelectionTarget = null
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't preview that CSV file.",
    }
  }
}

async function showMorePreviewLines(): Promise<void> {
  if (!state.pipelinePreview) {
    return
  }

  const nextRows = Math.min(
    state.pipelinePreviewRowsRequested + PREVIEW_PAGE_SIZE,
    state.pipelinePreview.total_lines
  )
  await loadPipelinePreview(state.pipelineForm.filePath, nextRows)
}

async function browseForCsvPath(): Promise<void> {
  try {
    const dialog = await import("@tauri-apps/plugin-dialog")
    const selection = await dialog.open({
      directory: false,
      multiple: false,
      filters: [{ name: "CSV files", extensions: ["csv", "txt"] }],
    })

    if (typeof selection !== "string" || !selection) {
      return
    }

    updatePipelineField("file_path", selection)
    if (!state.pipelineForm.name.trim()) {
      state.pipelineForm.name = basename(selection).replace(/\.[^.]+$/, "")
    }

    await loadPipelinePreview(selection)
  } catch {
    state.pipelineFeedback = {
      tone: "info",
      message:
        "The native file picker is only available in the desktop app. Enter the CSV path manually if you're using the browser preview.",
    }
  }
}

async function submitAuthConfig(formId: "welcome-form" | "settings-form"): Promise<void> {
  if (state.authSubmitting) {
    return
  }

  const payload = normalizeServerDraft()
  setServerDraft(payload)

  const feedbackKey = fieldFormFeedbackTarget(formId)

  state[feedbackKey] = null
  resetStateAuthFieldStates(payload.auth_type)

  if (!validateAuthFieldsForSubmit(payload, markField)) {
    return
  }

  try {
    await runAuthSubmission({
      render: () => undefined,
      setSubmitting: (value) => {
        state.authSubmitting = value
      },
      action: async () => {
        const urlValidation = await validateServerUrl(payload.url)
        if (!urlValidation.ok) {
          clearAuthValidationCache()
          markField("url", "invalid", urlValidation.message)
          state[feedbackKey] = {
            tone: "error",
            message: urlValidation.message,
          }
          return
        }

        markField("url", "valid")

        const result = await syncAuthenticationStatus(payload)
        applyConnectionValidationResult(payload, result, markField)
        if (!result.ok) {
          state[feedbackKey] = { tone: "error", message: result.message }
          return
        }

        state.config = await updateServerConfig(payload)
        state.authDraft = {
          ...emptyServerConfig(),
          ...state.config.server,
        }
        await syncAuthenticationStatus(state.config.server)
        await loadDatastreams()
        state[feedbackKey] = { tone: "success", message: result.message }
        state.settingsEditMode = false

        if (state.jobs.length === 0) {
          navigate("jobs-new")
        } else {
          navigate("dashboard")
        }
        syncRouteState()
      },
    })
  } catch (error) {
    clearAuthValidationCache()
    state[feedbackKey] = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't verify the HydroServer connection.",
    }
    state.lastConnectionState = "error"
  }
}

async function disconnectHydroServer(): Promise<void> {
  try {
    state.config = await clearServerConfig()
    state.authDraft = emptyServerConfig()
    state.connectionSummary = null
    state.lastConnectionState = "not_configured"
    state.datastreams = []
    state.datastreamsError = null
    state.welcomeFeedback = null
    state.settingsFeedback = null
    state.settingsEditMode = false
    resetStateAuthFieldStates("apikey")
    clearAuthValidationCache()
    navigate("welcome")
    syncRouteState()
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't disconnect from HydroServer right now.",
    }
  }
}

function resetPipelineEditorState(): void {
  state.pipelineForm = createEmptyPipelineForm()
  state.pipelinePreview = null
  state.pipelineSelectionTarget = null
  state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
  state.pipelineErrors = []
}

async function submitPipeline(): Promise<void> {
  if (!state.pipelinePreview) {
    await loadPipelinePreview(state.pipelineForm.filePath)
    return
  }

  state.pipelineErrors = validatePipeline()

  if (state.pipelineErrors.length > 0) {
    state.pipelineFeedback = {
      tone: "error",
      message: `${APP_NAME} needs a little more information before it can save this pipeline.`,
    }
    return
  }

  const mappedColumns = state.pipelineForm.mappings
    .filter((mapping) => mapping.datastreamId)
    .map((mapping) => {
      const datastream = state.datastreams.find(
        (item) => item.id === mapping.datastreamId
      )
      return {
        csv_column: mapping.csvColumn,
        datastream_id: mapping.datastreamId,
        datastream_name: datastream?.name ?? mapping.datastreamId,
      }
    })

  try {
    const created = await createJob({
      name: state.pipelineForm.name.trim(),
      enabled: true,
      file_path: state.pipelineForm.filePath.trim(),
      schedule_minutes: state.pipelineForm.scheduleMinutes,
      file_config: {
        header_row: state.pipelineForm.hasHeaderRow ? state.pipelineForm.headerRow : 0,
        data_start_row: state.pipelineForm.dataStartRow,
        delimiter: state.pipelineForm.delimiter,
        timestamp_column: state.pipelineForm.timestampColumn,
        timestamp_format: state.pipelineForm.timestampFormat,
        timezone: state.pipelineForm.timezone,
      },
      column_mappings: mappedColumns,
    })

    state.jobs = [...state.jobs, created]
    resetPipelineEditorState()
    state.pipelineFeedback = { tone: "success", message: "Pipeline saved." }
    navigate("dashboard")
    syncRouteState()
  } catch (error) {
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't save that pipeline.",
    }
  }
}

function changeCredentials(): void {
  state.authDraft = {
    ...emptyServerConfig(),
    ...(state.config?.server ?? {}),
  }
  state.settingsEditMode = true
  navigate("settings")
  syncRouteState()
}

function cancelCredentialEdit(): void {
  state.authDraft = {
    ...emptyServerConfig(),
    ...(state.config?.server ?? {}),
  }
  state.settingsEditMode = false
}

async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

async function handleToggleJob(jobId: string): Promise<void> {
  const job = state.jobs.find((item) => item.id === jobId)
  if (!job) {
    return
  }

  try {
    if (job.enabled) {
      await disableJob(jobId)
    } else {
      await enableJob(jobId)
    }

    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

async function handleDeleteJob(jobId: string): Promise<void> {
  if (!window.confirm("Delete this pipeline?")) {
    return
  }

  try {
    await deleteJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

let initialized = false

function init(): void {
  if (initialized) {
    return
  }

  initialized = true

  window.addEventListener("hashchange", () => {
    state.settingsFeedback = null
    syncRouteState()
  })

  window.setInterval(() => {
    void refreshJobs()
  }, 30_000)

  syncRouteState()
  void bootstrap()
}

const model = {
  state,
  APP_NAME,
  API_KEY_DOCS_URL,
  PREVIEW_PAGE_SIZE,
  init,
  bootstrap,
  isConnected,
  showSidebar,
  useWelcomeSurface,
  previewHeaders,
  parsedPreviewRows,
  activePreviewRowTarget,
  previewHandleLine,
  activeTimestampColumn,
  previewColumnClass,
  previewFieldClass,
  previewGuidanceText,
  canShowMorePreviewLines,
  connectionIndicator,
  onboardingRoute,
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
  handleRunJob,
  handleToggleJob,
  handleDeleteJob,
} as const

export function useAppModel() {
  return model
}
