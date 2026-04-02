import "./generated.css"
import appIconUrl from "../icons/icon-color.svg"

import {
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
  type AppConfig,
  type AuthType,
  type ConnectionState,
  type ConnectionTestResponse,
  type CsvPreviewResponse,
  type DatastreamSummary,
  type HealthResponse,
  type JobSummary,
  type ServerConfig,
} from "./api"
import { getRouteFromHash, navigate, routeHref, type AppRoute } from "./router"
import { formatRelativeTime, formatSchedule, shortenPath } from "./time"

const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key"
const APP_NAME = "HydroServer Streaming Data Loader"
const STARTUP_RETRY_ATTEMPTS = 12
const STARTUP_RETRY_DELAY_MS = 350

type Feedback = {
  tone: "success" | "error" | "info"
  message: string
} | null

type AuthFieldName = "url" | "api_key" | "username" | "password"

type FieldValidationState = {
  state: "idle" | "checking" | "valid" | "invalid"
  message: string | null
}

type PipelineMappingDraft = {
  csvColumn: string
  datastreamId: string
}

type PipelineFormState = {
  name: string
  filePath: string
  scheduleMinutes: number
  headerRow: number
  dataStartRow: number
  delimiter: string
  timestampColumn: string
  timestampFormat: string
  timezone: string
  mappings: PipelineMappingDraft[]
}

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
}

const shellElements = {
  sidebar: document.querySelector<HTMLElement>("#app-sidebar"),
  mainContent: document.querySelector<HTMLElement>("#main-content"),
  jobsLink: document.querySelector<HTMLAnchorElement>('[data-route="dashboard"]'),
  settingsLink: document.querySelector<HTMLAnchorElement>('[data-route="settings"]'),
  connectionDot: document.querySelector<HTMLElement>("#connection-status-dot"),
}

if (
  !shellElements.sidebar ||
  !shellElements.mainContent ||
  !shellElements.jobsLink ||
  !shellElements.settingsLink ||
  !shellElements.connectionDot
) {
  throw new Error("App shell is missing required elements.")
}

const { sidebar, mainContent, jobsLink, settingsLink, connectionDot } = shellElements

function createEmptyPipelineForm(): PipelineFormState {
  return {
    name: "",
    filePath: "",
    scheduleMinutes: 15,
    headerRow: 3,
    dataStartRow: 4,
    delimiter: ",",
    timestampColumn: "Timestamp",
    timestampFormat: "%Y-%m-%d %H:%M:%S",
    timezone: "America/Denver",
    mappings: [],
  }
}

const state: UiState = {
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
  authFieldStates: {
    url: { state: "idle", message: null },
    api_key: { state: "idle", message: null },
    username: { state: "idle", message: null },
    password: { state: "idle", message: null },
  },
  authSubmitting: false,
  lastAuthValidationServer: null,
  lastAuthValidationResult: null,
}

let authValidationRequestId = 0

function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
  }
}

window.setInterval(() => {
  void refreshJobs()
  render()
}, 30_000)

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;")
}

function feedbackMarkup(feedback: Feedback): string {
  if (!feedback) {
    return ""
  }

  const toneClass =
    feedback.tone === "success"
      ? "notice-success"
      : feedback.tone === "error"
        ? "notice-error"
        : "notice-info"

  return `<div class="${toneClass}">${escapeHtml(feedback.message)}</div>`
}

function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean)
  return segments.at(-1) ?? path
}

function connected(): boolean {
  return state.connectionSummary?.ok === true && state.lastConnectionState === "connected"
}

function currentServerConfig(): ServerConfig {
  return state.authDraft
}

function emptyFieldValidationState(): FieldValidationState {
  return { state: "idle", message: null }
}

function resetAuthFieldStates(authType: AuthType): void {
  state.authFieldStates.url = emptyFieldValidationState()
  state.authFieldStates.api_key = emptyFieldValidationState()
  state.authFieldStates.username = emptyFieldValidationState()
  state.authFieldStates.password = emptyFieldValidationState()

  if (authType === "apikey") {
    state.authFieldStates.username = emptyFieldValidationState()
    state.authFieldStates.password = emptyFieldValidationState()
  } else {
    state.authFieldStates.api_key = emptyFieldValidationState()
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

function readServerConfigForm(
  form: HTMLFormElement,
  base: ServerConfig = currentServerConfig()
): ServerConfig {
  const data = new FormData(form)
  const authType = data.get("auth_type") === "userpass" ? "userpass" : "apikey"

  return {
    auth_type: authType,
    url: String(data.get("url") ?? "").trim(),
    api_key: authType === "apikey" ? String(data.get("api_key") ?? "").trim() : base.api_key,
    username:
      authType === "userpass" ? String(data.get("username") ?? "").trim() : base.username,
    password:
      authType === "userpass" ? String(data.get("password") ?? "").trim() : base.password,
  }
}

function setServerDraft(server: ServerConfig): void {
  state.authDraft = { ...server }
}

function sameServerConfig(left: ServerConfig | null, right: ServerConfig): boolean {
  if (!left) {
    return false
  }

  return (
    left.auth_type === right.auth_type &&
    left.url === right.url &&
    left.api_key === right.api_key &&
    left.username === right.username &&
    left.password === right.password
  )
}

function markField(
  field: AuthFieldName,
  nextState: FieldValidationState["state"],
  message: string | null = null
): void {
  state.authFieldStates[field] = { state: nextState, message }
}

function credentialFields(authType: AuthType): AuthFieldName[] {
  return authType === "userpass" ? ["username", "password"] : ["api_key"]
}

function authFieldStateMarkup(field: AuthFieldName): string {
  const fieldState = state.authFieldStates[field]

  if (fieldState.state === "valid") {
    return '<span class="input-status input-status-valid" aria-hidden="true">&#10003;</span>'
  }

  if (fieldState.state === "invalid") {
    return '<span class="input-status input-status-invalid" aria-hidden="true">&#10005;</span>'
  }

  if (fieldState.state === "checking") {
    return '<span class="input-status input-status-checking" aria-hidden="true"><span class="input-spinner"></span></span>'
  }

  return ""
}

function authFieldErrorMarkup(field: AuthFieldName): string {
  const fieldState = state.authFieldStates[field]
  if (fieldState.state !== "invalid" || !fieldState.message) {
    return ""
  }

  return `<p class="field-error">${escapeHtml(fieldState.message)}</p>`
}

function renderAuthInputField(params: {
  label: string
  name: AuthFieldName
  type: "url" | "text" | "password"
  value: string
  placeholder: string
  helpText?: string
}): string {
  const { label, name, type, value, placeholder, helpText } = params

  return `
    <label class="field">
      <span class="label">${escapeHtml(label)}</span>
      <span class="field-control">
        <input class="input input-with-status" type="${type}" name="${name}" value="${escapeHtml(value)}" placeholder="${escapeHtml(placeholder)}" />
        ${authFieldStateMarkup(name)}
      </span>
      ${helpText ? `<p class="field-hint">${escapeHtml(helpText)}</p>` : ""}
      ${authFieldErrorMarkup(name)}
    </label>
  `
}

function fieldFormFeedbackTarget(formId: string): "welcomeFeedback" | "settingsFeedback" {
  return formId === "welcome-form" ? "welcomeFeedback" : "settingsFeedback"
}

function clearAuthFormFeedback(formId: string): void {
  state[fieldFormFeedbackTarget(formId)] = null
}

function clearAuthValidationCache(): void {
  state.lastAuthValidationServer = null
  state.lastAuthValidationResult = null
}

function setAuthFieldLoading(server: ServerConfig): void {
  markField("url", "checking")
  for (const field of credentialFields(server.auth_type)) {
    markField(field, "checking")
  }
}

function isValidHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value)
    return parsed.protocol === "http:" || parsed.protocol === "https:"
  } catch {
    return false
  }
}

function applyConnectionValidationResult(server: ServerConfig, result: ConnectionTestResponse): void {
  markField("url", "valid")

  if (result.ok) {
    for (const field of credentialFields(server.auth_type)) {
      markField(field, "valid")
    }
    return
  }

  const message = result.message
  const isUrlError =
    result.message.includes("Couldn't reach HydroServer") ||
    result.message.includes("HydroServer returned an error")

  if (isUrlError) {
    markField("url", "invalid", message)
    for (const field of credentialFields(server.auth_type)) {
      markField(field, "idle")
    }
    return
  }

  for (const field of credentialFields(server.auth_type)) {
    markField(field, "invalid", message)
  }
}

async function validateAuthField(form: HTMLFormElement, field: AuthFieldName): Promise<void> {
  const server = readServerConfigForm(form)
  const requestId = ++authValidationRequestId
  setServerDraft(server)

  if (field === "url") {
    if (!server.url) {
      markField("url", "invalid", "Enter the HydroServer URL.")
      render()
      return
    }

    if (!isValidHttpUrl(server.url)) {
      markField("url", "invalid", "Enter a full http:// or https:// URL.")
      render()
      return
    }
  }

  if (field === "api_key" && server.auth_type === "apikey" && !server.api_key) {
    markField("api_key", "invalid", "Enter the API key.")
    render()
    return
  }

  if (field === "username" && server.auth_type === "userpass" && !server.username) {
    markField("username", "invalid", "Enter the username.")
    render()
    return
  }

  if (field === "password" && server.auth_type === "userpass" && !server.password) {
    markField("password", "invalid", "Enter the password.")
    render()
    return
  }

  if (field === "url") {
    markField("url", "valid")
  } else {
    markField(field, "checking")
  }

  const requiredFieldsReady =
    server.auth_type === "apikey"
      ? Boolean(server.url && isValidHttpUrl(server.url) && server.api_key)
      : Boolean(server.url && isValidHttpUrl(server.url) && server.username && server.password)

  if (!requiredFieldsReady) {
    render()
    return
  }

  for (const name of credentialFields(server.auth_type)) {
    markField(name, "checking")
  }
  markField("url", "checking")
  render()

  try {
    const result = await testConnection(server)

    if (requestId !== authValidationRequestId) {
      return
    }

    state.lastAuthValidationServer = server
    state.lastAuthValidationResult = result
    applyConnectionValidationResult(server, result)
  } catch (error) {
    if (requestId !== authValidationRequestId) {
      return
    }

    clearAuthValidationCache()
    const message =
      error instanceof Error ? error.message : "Couldn't test the HydroServer connection."
    const isUrlError =
      message.includes("Request failed with status 500") ||
      message.includes("Failed to fetch") ||
      message.includes("Couldn't test the HydroServer connection.")

    if (isUrlError) {
      markField("url", "invalid", message)
      for (const name of credentialFields(server.auth_type)) {
        markField(name, "idle")
      }
    } else {
      markField("url", "valid")
      for (const name of credentialFields(server.auth_type)) {
        markField(name, "invalid", message)
      }
    }
  }

  render()
}

function previewHeaders(): string[] {
  return state.pipelinePreview?.parsed_rows[0] ?? []
}

function pipelineMappingsByColumn(): Map<string, string> {
  return new Map(state.pipelineForm.mappings.map(mapping => [mapping.csvColumn, mapping.datastreamId]))
}

function previewColumnClass(columnName: string): string {
  if (columnName === state.pipelineForm.timestampColumn) {
    return "preview-col-timestamp"
  }

  const mapped = state.pipelineForm.mappings.find(
    mapping => mapping.csvColumn === columnName && mapping.datastreamId
  )
  return mapped ? "preview-col-mapped" : ""
}

function initializeMappings(headers: string[]): void {
  const existing = pipelineMappingsByColumn()
  state.pipelineForm.mappings = headers
    .filter(header => header !== state.pipelineForm.timestampColumn)
    .map(header => ({
      csvColumn: header,
      datastreamId: existing.get(header) ?? "",
    }))
}

function applyPreview(path: string, preview: CsvPreviewResponse): void {
  state.pipelinePreview = preview
  state.pipelineForm.filePath = path
  state.pipelineForm.headerRow = preview.detected_header_row ?? state.pipelineForm.headerRow
  state.pipelineForm.dataStartRow = preview.detected_data_start_row ?? state.pipelineForm.dataStartRow
  state.pipelineForm.delimiter = preview.detected_delimiter || state.pipelineForm.delimiter

  const headers = preview.parsed_rows[0] ?? []
  if (headers.length > 0) {
    const preferredTimestamp =
      headers.find(header => header.toLowerCase().includes("time")) ?? headers[0]
    state.pipelineForm.timestampColumn = headers.includes(state.pipelineForm.timestampColumn)
      ? state.pipelineForm.timestampColumn
      : preferredTimestamp
  }

  if (!state.pipelineForm.name.trim()) {
    const inferred = basename(path).replace(/\.[^.]+$/, "")
    state.pipelineForm.name = inferred
  }

  initializeMappings(headers)
}

function connectionIndicator(): { label: string; className: string } {
  if (!serverConfigured(state.config?.server)) {
    return { label: "HydroServer not configured", className: "status-dot bg-slate-300" }
  }

  if (connected()) {
    return { label: "Connected to HydroServer", className: "status-dot bg-emerald-500" }
  }

  if (state.lastConnectionState === "error") {
    return { label: "HydroServer authentication error", className: "status-dot bg-rose-500" }
  }

  return { label: "HydroServer configured", className: "status-dot bg-sky-500" }
}

function statusPill(job: JobSummary): string {
  const classes: Record<JobSummary["status"], string> = {
    healthy: "pill-success",
    warning: "pill-warning",
    error: "pill-danger",
    disabled: "pill-muted",
    pending: "pill-info",
    running: "pill-info",
  }

  return `<span class="${classes[job.status]}">${escapeHtml(job.status_message)}</span>`
}

function renderConnectedCard(showActions: boolean): string {
  if (!connected() || !state.connectionSummary) {
    return ""
  }

  const datastreamText =
    state.connectionSummary.datastream_count === 1
      ? "1 datastream available"
      : `${state.connectionSummary.datastream_count} datastreams available`

  return `
    <article class="summary-card">
      <div class="summary-card-copy">
        <p class="eyebrow">Authenticated</p>
        <h2 class="section-title">${escapeHtml(
          state.connectionSummary.instance_name ?? "HydroServer"
        )}</h2>
        <p class="section-copy">${escapeHtml(state.connectionSummary.message)}</p>
        <div class="summary-inline">
          <span class="pill-success">Connected</span>
          <span class="summary-meta">${escapeHtml(datastreamText)}</span>
        </div>
      </div>
      ${
        showActions
          ? `
        <div class="button-row">
          <button class="btn-ghost" type="button" data-action="change-credentials">Change credentials</button>
          ${
            state.jobs.length === 0
              ? `<a class="btn-primary" href="${routeHref("jobs-new")}">Create first pipeline</a>`
              : ""
          }
        </div>
      `
          : ""
      }
    </article>
  `
}

function renderAuthForm(
  formId: "welcome-form" | "settings-form",
  feedback: Feedback,
  submitLabel: string,
  secondaryAction: string
): string {
  const server = currentServerConfig()
  const usingUserPass = server.auth_type === "userpass"
  const authToggleLabel = usingUserPass
    ? "Connect with an API key"
    : "Connect with username and password"
  const submitDisabled = state.authSubmitting ? "disabled" : ""
  const submitLabelText = state.authSubmitting ? "Connecting..." : submitLabel

  return `
    <form id="${formId}" class="auth-card" autocomplete="off">
      <section class="card-section">
        <div class="auth-header">
          <img class="auth-app-icon" src="${appIconUrl}" alt="HydroServer Streaming Data Loader icon" />
          <h1 class="page-title">Connect to your HydroServer instance</h1>
        </div>

        ${feedbackMarkup(feedback)}
        <input type="hidden" name="auth_type" value="${server.auth_type}" />

        ${renderAuthInputField({
          label: "Host URL",
          name: "url",
          type: "url",
          value: server.url,
          placeholder: "https://playground.hydroserver.org",
        })}

        ${
          usingUserPass
            ? `
              ${renderAuthInputField({
                label: "Username",
                name: "username",
                type: "text",
                value: server.username,
                placeholder: "name@example.com",
              })}
              ${renderAuthInputField({
                label: "Password",
                name: "password",
                type: "password",
                value: server.password,
                placeholder: "Enter your HydroServer password",
              })}
            `
            : `
              ${renderAuthInputField({
                label: "API key",
                name: "api_key",
                type: "password",
                value: server.api_key,
                placeholder: "KaTz74swGqHn__I2VY6ceIzrIxC04oDhUrLLgBTH9ACxYIunmkrdmqk",
              })}

              <a class="text-link" href="${API_KEY_DOCS_URL}" target="_blank" rel="noreferrer">
                How to create an API key
              </a>
            `
        }

        <div class="auth-toggle-group">
          <span class="auth-divider-label">or</span>

          <button class="auth-toggle" type="button" data-action="toggle-auth-mode">
            ${escapeHtml(authToggleLabel)}
          </button>
        </div>

        <div class="button-row button-row-end">
          ${secondaryAction}
          <button class="btn-primary" type="submit" ${submitDisabled}>${escapeHtml(submitLabelText)}</button>
        </div>
      </section>
    </form>
  `
}

function renderWelcome(): string {
  return `
    <section class="welcome-shell animate-fade-in">
      ${renderAuthForm("welcome-form", state.welcomeFeedback, "Connect to HydroServer", "")}
    </section>
  `
}

function renderSettings(): string {
  const showForm = !connected() || state.settingsEditMode

  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Settings</p>
          <h1 class="page-title">HydroServer connection</h1>
          <p class="page-copy">After ${APP_NAME} is connected, this form stays out of the way. You can return here any time to rotate credentials or verify access again.</p>
        </div>
      </header>

      ${showForm ? renderAuthForm("settings-form", state.settingsFeedback, "Save and verify", connected() ? '<button class="btn-ghost" type="button" data-action="cancel-credential-edit">Cancel</button>' : "") : renderConnectedCard(true)}
    </section>
  `
}

function renderDashboard(): string {
  if (state.jobs.length === 0) {
    return `
      <section class="page-shell animate-fade-in">
        <header class="page-header">
          <div>
            <p class="eyebrow">Dashboard</p>
            <h1 class="page-title">Jobs</h1>
            <p class="page-copy">Finish the onboarding flow by creating your first pipeline. ${APP_NAME} will use that saved local configuration from then on.</p>
          </div>
          <a class="btn-primary" href="${routeHref("jobs-new")}">Create first pipeline</a>
        </header>
      </section>
    `
  }

  const cards = state.jobs
    .map(job => {
      const lastLine = job.last_error
        ? `Failed ${formatRelativeTime(job.last_run_at)}`
        : `Last pushed ${formatRelativeTime(job.last_pushed_timestamp)}`

      return `
        <article class="job-card animate-fade-in">
          <div class="job-card-top">
            <div>
              <div class="job-card-title-row">
                <span class="status-dot ${job.status === "error" ? "bg-rose-500" : job.status === "warning" ? "bg-amber-500" : job.status === "disabled" ? "bg-slate-300" : "bg-emerald-500"}"></span>
                <h2 class="section-title">${escapeHtml(job.name)}</h2>
              </div>
              <p class="section-copy">${escapeHtml(shortenPath(job.file_path))}</p>
              <p class="job-meta ${job.status === "error" ? "text-rose-600" : ""}">
                ${escapeHtml(lastLine)} · ${escapeHtml(formatSchedule(job.schedule_minutes))}
              </p>
            </div>
            ${statusPill(job)}
          </div>

          <div class="job-card-actions">
            <button class="btn-ghost" data-action="run-job" data-job-id="${job.id}">Run now</button>
            <button class="btn-ghost" data-action="toggle-job" data-job-id="${job.id}">
              ${job.enabled ? "Disable" : "Enable"}
            </button>
            <button class="btn-danger" data-action="delete-job" data-job-id="${job.id}">Delete</button>
          </div>
        </article>
      `
    })
    .join("")

  return `
    <section class="page-shell">
      <header class="page-header">
        <div>
          <p class="eyebrow">Dashboard</p>
          <h1 class="page-title">Pipelines</h1>
          <p class="page-copy">Your saved pipelines watch local CSV sources, track row cursors, and push only new observations into HydroServer.</p>
        </div>
        <a class="btn-primary" href="${routeHref("jobs-new")}">Add pipeline</a>
      </header>
      <div class="card-stack">${cards}</div>
    </section>
  `
}

function renderPipelinePreview(): string {
  if (!state.pipelinePreview) {
    return `
      <article class="preview-card">
        <div class="preview-placeholder">
          <div class="empty-icon">CSV</div>
          <h2 class="section-title">Preview a source file</h2>
          <p class="section-copy">Choose a CSV file path, then load the preview to detect headers and map source columns to HydroServer datastreams.</p>
        </div>
      </article>
    `
  }

  const headers = previewHeaders()
  const parsedRows = state.pipelinePreview.parsed_rows.slice(1, 7)
  const rawRows = state.pipelinePreview.raw_lines
    .map((line, index) => {
      const lineNumber = index + 1
      const rowClass =
        lineNumber === state.pipelineForm.headerRow
          ? "preview-raw-line preview-raw-line-header"
          : lineNumber === state.pipelineForm.dataStartRow
            ? "preview-raw-line preview-raw-line-data"
            : "preview-raw-line"

      return `
        <div class="${rowClass}">
          <span class="preview-line-number">${lineNumber}</span>
          <code>${escapeHtml(line)}</code>
        </div>
      `
    })
    .join("")

  const headerCells = headers
    .map(
      header =>
        `<th class="preview-cell ${previewColumnClass(header)}">${escapeHtml(header)}</th>`
    )
    .join("")

  const tableRows = parsedRows
    .map(
      row => `
        <tr>
          ${row
            .map((cell, index) => {
              const columnName = headers[index] ?? ""
              return `<td class="preview-cell ${previewColumnClass(columnName)}">${escapeHtml(cell)}</td>`
            })
            .join("")}
        </tr>
      `
    )
    .join("")

  return `
    <article class="preview-card">
      <div class="preview-header">
        <div>
          <p class="eyebrow">Preview</p>
          <h2 class="section-title">${escapeHtml(basename(state.pipelineForm.filePath))}</h2>
        </div>
        <div class="preview-summary">
          <span class="pill-info">Header row ${state.pipelineForm.headerRow}</span>
          <span class="pill-info">Data starts ${state.pipelineForm.dataStartRow}</span>
          <span class="pill-info">${escapeHtml(state.pipelinePreview.encoding)}</span>
        </div>
      </div>

      <div class="preview-raw">${rawRows}</div>

      <div class="preview-table-shell">
        <table class="preview-table">
          <thead>
            <tr>${headerCells}</tr>
          </thead>
          <tbody>
            ${tableRows}
          </tbody>
        </table>
      </div>

      <footer class="preview-footer">
        Showing the first ${Math.min(state.pipelinePreview.total_lines, state.pipelinePreview.raw_lines.length)} of ${state.pipelinePreview.total_lines} lines
      </footer>
    </article>
  `
}

function renderPipelineMappings(): string {
  const availableMappings = state.pipelineForm.mappings

  if (!state.pipelinePreview || availableMappings.length === 0) {
    return `
      <div class="pipeline-subcard">
        <h3 class="section-title">Column mappings</h3>
        <p class="section-copy">Load a CSV preview first so HydroServer Streaming Data Loader can list the available source columns.</p>
      </div>
    `
  }

  const rows = availableMappings
    .map(mapping => {
      const options = [
        `<option value="">Not mapped</option>`,
        ...state.datastreams.map(
          datastream =>
            `<option value="${escapeHtml(datastream.id)}" ${
              datastream.id === mapping.datastreamId ? "selected" : ""
            }>${escapeHtml(datastream.name)}</option>`
        ),
      ].join("")

      return `
        <div class="mapping-row">
          <div>
            <p class="mapping-source">${escapeHtml(mapping.csvColumn)}</p>
            <p class="mapping-help">Source column</p>
          </div>
          <select class="input" data-mapping-column="${escapeHtml(mapping.csvColumn)}">
            ${options}
          </select>
        </div>
      `
    })
    .join("")

  return `
    <div class="pipeline-subcard">
      <h3 class="section-title">Column mappings</h3>
      <p class="section-copy">Map each source column to a HydroServer datastream. Leave any unused source columns as “Not mapped.”</p>
      <div class="mapping-grid">${rows}</div>
    </div>
  `
}

function renderPipelineEditor(): string {
  if (!connected()) {
    return renderWelcome()
  }

  if (state.datastreamsError) {
    return `
      <section class="page-shell animate-fade-in">
        <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">HydroServer access needs attention</h1>
          <p class="page-copy">${APP_NAME} authenticated successfully, but it could not load the target datastreams needed for mapping.</p>
        </div>
      </header>

        ${renderConnectedCard(true)}
        <div class="notice-error">${escapeHtml(state.datastreamsError)}</div>
      </section>
    `
  }

  if (state.datastreams.length === 0) {
    return `
      <section class="page-shell animate-fade-in">
        <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">No datastreams are available yet</h1>
          <p class="page-copy">Create at least one target datastream in HydroServer first, then come back and ${APP_NAME} will use it for column mapping.</p>
        </div>
      </header>

        ${renderConnectedCard(true)}
        <a class="btn-link" href="${API_KEY_DOCS_URL}" target="_blank" rel="noreferrer">
          Open the HydroServer 101 tutorial
        </a>
      </section>
    `
  }

  const timestampOptions = previewHeaders()
    .map(
      header =>
        `<option value="${escapeHtml(header)}" ${
          header === state.pipelineForm.timestampColumn ? "selected" : ""
        }>${escapeHtml(header)}</option>`
    )
    .join("")

  const pipelineErrorMarkup =
    state.pipelineErrors.length > 0
      ? `
        <div class="validation-panel">
          <h3 class="section-title">Fix these issues before saving</h3>
          <ul class="validation-list">
            ${state.pipelineErrors.map(error => `<li>${escapeHtml(error)}</li>`).join("")}
          </ul>
        </div>
      `
      : ""

  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">Connect a CSV source to HydroServer</h1>
          <p class="page-copy">Choose the CSV file you want ${APP_NAME} to watch, preview the source rows, then map the source columns to your HydroServer datastreams.</p>
        </div>
      </header>

      ${renderConnectedCard(true)}

      <div class="pipeline-layout">
        <form id="pipeline-form" class="pipeline-form" autocomplete="off">
          <div class="pipeline-subcard">
            <h2 class="section-title">Pipeline details</h2>

            <label class="field">
              <span class="label">Pipeline name</span>
              <input class="input" type="text" name="pipeline_name" value="${escapeHtml(
                state.pipelineForm.name
              )}" placeholder="Little Bear River stage" />
            </label>

            <label class="field">
              <span class="label">Watched CSV file path</span>
              <input class="input" type="text" name="file_path" value="${escapeHtml(
                state.pipelineForm.filePath
              )}" placeholder="/Users/you/datalogger/output.csv" />
              <span class="field-hint">${APP_NAME} stores the watched file path locally so it can keep loading new rows in the background.</span>
            </label>

            <div class="button-row">
              <button class="btn-ghost" type="button" data-action="browse-csv">Browse for CSV</button>
              <button class="btn-ghost" type="button" data-action="load-preview">Load preview</button>
            </div>

            <label class="field">
              <span class="label">Schedule</span>
              <select class="input" name="schedule_minutes">
                ${[5, 15, 30, 60]
                  .map(
                    minutes =>
                      `<option value="${minutes}" ${
                        state.pipelineForm.scheduleMinutes === minutes ? "selected" : ""
                      }>Every ${formatSchedule(minutes).replace("Every ", "")}</option>`
                  )
                  .join("")}
              </select>
            </label>
          </div>

          <div class="pipeline-subcard">
            <h2 class="section-title">File structure</h2>

            <div class="split-fields">
              <label class="field">
                <span class="label">Header row</span>
                <input class="input" type="number" min="1" name="header_row" value="${state.pipelineForm.headerRow}" />
              </label>

              <label class="field">
                <span class="label">Data start row</span>
                <input class="input" type="number" min="1" name="data_start_row" value="${state.pipelineForm.dataStartRow}" />
              </label>
            </div>

            <div class="split-fields">
              <label class="field">
                <span class="label">Delimiter</span>
                <input class="input" type="text" name="delimiter" value="${escapeHtml(
                  state.pipelineForm.delimiter
                )}" maxlength="2" />
              </label>

              <label class="field">
                <span class="label">Timezone</span>
                <input class="input" type="text" name="timezone" value="${escapeHtml(
                  state.pipelineForm.timezone
                )}" />
              </label>
            </div>

            <label class="field">
              <span class="label">Timestamp column</span>
              ${
                previewHeaders().length > 0
                  ? `<select class="input" name="timestamp_column">${timestampOptions}</select>`
                  : `<input class="input" type="text" name="timestamp_column" value="${escapeHtml(
                      state.pipelineForm.timestampColumn
                    )}" placeholder="Timestamp" />`
              }
            </label>

            <label class="field">
              <span class="label">Timestamp format</span>
              <input class="input" type="text" name="timestamp_format" value="${escapeHtml(
                state.pipelineForm.timestampFormat
              )}" placeholder="%Y-%m-%d %H:%M:%S" />
            </label>
          </div>

          ${renderPipelineMappings()}
          ${pipelineErrorMarkup}
          ${feedbackMarkup(state.pipelineFeedback)}

          <div class="button-row">
            <button class="btn-primary" type="submit">Save pipeline</button>
          </div>
        </form>

        ${renderPipelinePreview()}
      </div>
    </section>
  `
}

function renderFatalError(): string {
  return `
    <section class="welcome-shell animate-fade-in">
      <div class="welcome-card">
        <p class="eyebrow">Sidecar error</p>
        <h1 class="page-title">The background process is unavailable</h1>
        <p class="page-copy">${escapeHtml(state.bootstrapError ?? `${APP_NAME} could not reach the local background service.`)}</p>
        <button class="btn-primary" type="button" data-action="retry-bootstrap">Retry</button>
      </div>
    </section>
  `
}

function render(): void {
  state.route = getRouteFromHash()

  let currentRoute = getRouteFromHash()

  if (!state.loading && !state.bootstrapError) {
    if (!connected() && currentRoute !== "settings" && currentRoute !== "welcome") {
      navigate("welcome")
      currentRoute = "welcome"
    } else if (connected() && state.jobs.length === 0 && (currentRoute === "dashboard" || currentRoute === "welcome")) {
      navigate("jobs-new")
      currentRoute = "jobs-new"
    }
  }

  const showSidebar = currentRoute !== "welcome" && !state.bootstrapError
  sidebar.classList.toggle("hidden", !showSidebar)

  jobsLink.className = currentRoute === "dashboard" ? "nav-item nav-item-active" : "nav-item"
  settingsLink.className = currentRoute === "settings" ? "nav-item nav-item-active" : "nav-item"

  const status = connectionIndicator()
  connectionDot.className = status.className
  connectionDot.title = status.label

  if (state.loading) {
    mainContent.innerHTML = `
      <section class="welcome-shell">
        <div class="welcome-card">
          <p class="eyebrow">Starting Up</p>
          <h1 class="page-title">Loading local configuration</h1>
          <p class="page-copy">Connecting ${APP_NAME} to its local background service and validating your HydroServer configuration.</p>
        </div>
      </section>
    `
    return
  }

  if (state.bootstrapError) {
    mainContent.innerHTML = renderFatalError()
    return
  }

  if (currentRoute === "settings") {
    mainContent.innerHTML = renderSettings()
    return
  }

  if (currentRoute === "welcome") {
    mainContent.innerHTML = renderWelcome()
    return
  }

  if (currentRoute === "jobs-new") {
    mainContent.innerHTML = renderPipelineEditor()
    return
  }

  mainContent.innerHTML = renderDashboard()
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => window.setTimeout(resolve, ms))
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

async function loadInitialStateWithRetry(): Promise<{
  health: HealthResponse
  config: AppConfig
  jobs: JobSummary[]
}> {
  let lastError: unknown = null

  for (let attempt = 1; attempt <= STARTUP_RETRY_ATTEMPTS; attempt += 1) {
    try {
      const [health, config, jobs] = await Promise.all([getHealth(), getConfig(), listJobs()])
      return { health, config, jobs }
    } catch (error) {
      lastError = error

      if (attempt === STARTUP_RETRY_ATTEMPTS || !isTransientBootstrapError(error)) {
        throw error
      }

      await sleep(STARTUP_RETRY_DELAY_MS)
    }
  }

  throw lastError instanceof Error ? lastError : new Error(`Failed to load ${APP_NAME}.`)
}

async function syncAuthenticationStatus(
  server: ServerConfig,
  context: "bootstrap" | "welcome" | "settings"
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server)
  state.lastAuthValidationServer = server
  state.lastAuthValidationResult = result
  state.connectionSummary = result
  state.lastConnectionState = result.state

  if (result.ok) {
    await loadDatastreams()
  } else {
    state.datastreams = []
    state.datastreamsError = null
  }

  if (context === "bootstrap" && !result.ok) {
    state.welcomeFeedback = { tone: "error", message: result.message }
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

async function bootstrap(): Promise<void> {
  state.loading = true
  state.bootstrapError = null
  render()

  try {
    const { health, config, jobs } = await loadInitialStateWithRetry()
    state.health = health
    state.config = config
    state.jobs = jobs
    state.lastConnectionState = health.connection.state

    if (serverConfigured(config.server)) {
      await syncAuthenticationStatus(config.server, "bootstrap")
    }
  } catch (error) {
    state.bootstrapError = error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`
  } finally {
    state.loading = false
    render()
  }
}

async function refreshJobs(): Promise<void> {
  if (state.bootstrapError || state.loading) {
    return
  }

  try {
    state.jobs = await listJobs()
    render()
  } catch {
    // Keep existing UI state on polling failure.
  }
}

function updatePipelineField(name: string, value: string): void {
  switch (name) {
    case "pipeline_name":
      state.pipelineForm.name = value
      break
    case "file_path":
      state.pipelineForm.filePath = value
      break
    case "schedule_minutes":
      state.pipelineForm.scheduleMinutes = Number(value) || 15
      break
    case "header_row":
      state.pipelineForm.headerRow = Number(value) || 1
      break
    case "data_start_row":
      state.pipelineForm.dataStartRow = Number(value) || 1
      break
    case "delimiter":
      state.pipelineForm.delimiter = value || ","
      break
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value
      initializeMappings(previewHeaders())
      render()
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

function validatePipeline(): string[] {
  const errors: string[] = []
  const headers = previewHeaders()
  const selectedMappings = state.pipelineForm.mappings.filter(mapping => mapping.datastreamId)
  const datastreamIds = new Set(state.datastreams.map(datastream => datastream.id))
  const seenTargets = new Set<string>()

  if (!connected()) {
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

  if (state.pipelineForm.headerRow < 1) {
    errors.push("Header row must be 1 or greater.")
  }

  if (state.pipelineForm.dataStartRow <= state.pipelineForm.headerRow) {
    errors.push("Data start row must come after the header row.")
  }

  if (headers.length > 0 && !headers.includes(state.pipelineForm.timestampColumn)) {
    errors.push("Choose a timestamp column that exists in the previewed CSV header.")
  }

  if (selectedMappings.length === 0) {
    errors.push("Map at least one source column to a HydroServer datastream.")
  }

  for (const mapping of selectedMappings) {
    if (!datastreamIds.has(mapping.datastreamId)) {
      errors.push(`The selected target for ${mapping.csvColumn} is not a valid HydroServer datastream.`)
    }

    if (seenTargets.has(mapping.datastreamId)) {
      errors.push("Each target datastream can only be mapped once in this first-run flow.")
    }

    seenTargets.add(mapping.datastreamId)
  }

  return errors
}

async function loadPipelinePreview(path: string): Promise<void> {
  if (!path.trim()) {
    state.pipelineFeedback = { tone: "error", message: "Enter or choose a CSV file path first." }
    render()
    return
  }

  try {
    const preview = await getCsvPreview(path.trim())
    applyPreview(path.trim(), preview)
    state.pipelineErrors = []
    state.pipelineFeedback = {
      tone: "success",
      message: "Preview loaded. Review the detected structure and map the source columns.",
    }
  } catch (error) {
    state.pipelinePreview = null
    state.pipelineFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't preview that CSV file.",
    }
  }

  render()
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

    state.pipelineForm.filePath = selection
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
    render()
  }
}

async function saveAuthenticatedServerConfig(
  form: HTMLFormElement,
  context: "welcome" | "settings"
): Promise<void> {
  if (state.authSubmitting) {
    return
  }

  const payload = readServerConfigForm(form)
  setServerDraft(payload)

  const feedbackKey = context === "welcome" ? "welcomeFeedback" : "settingsFeedback"
  const canReuseValidation =
    sameServerConfig(state.lastAuthValidationServer, payload) &&
    state.lastAuthValidationResult?.ok === true

  try {
    state.authSubmitting = true
    setAuthFieldLoading(payload)
    render()

    const result = canReuseValidation
      ? state.lastAuthValidationResult!
      : await syncAuthenticationStatus(payload, context)

    if (canReuseValidation) {
      state.connectionSummary = result
      state.lastConnectionState = result.state
      await loadDatastreams()
    }

    applyConnectionValidationResult(payload, result)
    if (!result.ok) {
      state[feedbackKey] = { tone: "error", message: result.message }
      render()
      return
    }

    state.config = await updateServerConfig(payload)
    state.authDraft = {
      ...emptyServerConfig(),
      ...state.config.server,
    }
    state[feedbackKey] = { tone: "success", message: result.message }
    state.settingsEditMode = false

    if (state.jobs.length === 0) {
      navigate("jobs-new")
    } else {
      navigate("dashboard")
    }
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
  } finally {
    state.authSubmitting = false
  }

  render()
}

async function savePipeline(): Promise<void> {
  state.pipelineErrors = validatePipeline()

  if (state.pipelineErrors.length > 0) {
    state.pipelineFeedback = {
      tone: "error",
      message: `${APP_NAME} needs a little more information before it can save this pipeline.`,
    }
    render()
    return
  }

  const mappedColumns = state.pipelineForm.mappings
    .filter(mapping => mapping.datastreamId)
    .map(mapping => {
      const datastream = state.datastreams.find(item => item.id === mapping.datastreamId)
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
        header_row: state.pipelineForm.headerRow,
        data_start_row: state.pipelineForm.dataStartRow,
        delimiter: state.pipelineForm.delimiter,
        timestamp_column: state.pipelineForm.timestampColumn,
        timestamp_format: state.pipelineForm.timestampFormat,
        timezone: state.pipelineForm.timezone,
      },
      column_mappings: mappedColumns,
    })

    state.jobs = [...state.jobs, created]
    state.pipelineForm = createEmptyPipelineForm()
    state.pipelinePreview = null
    state.pipelineErrors = []
    state.pipelineFeedback = { tone: "success", message: "Pipeline saved." }
    navigate("dashboard")
  } catch (error) {
    state.pipelineFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't save that pipeline.",
    }
  }

  render()
}

window.addEventListener("hashchange", () => {
  state.settingsFeedback = null
  render()
})

mainContent.addEventListener("submit", event => {
  const target = event.target
  if (!(target instanceof HTMLFormElement)) {
    return
  }

  event.preventDefault()

  if (target.id === "welcome-form") {
    void saveAuthenticatedServerConfig(target, "welcome")
    return
  }

  if (target.id === "settings-form") {
    void saveAuthenticatedServerConfig(target, "settings")
    return
  }

  if (target.id === "pipeline-form") {
    void savePipeline()
  }
})

mainContent.addEventListener("input", event => {
  const target = event.target

  if (
    !(
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
  )
  ) {
    return
  }

  if (target.form?.id === "welcome-form" || target.form?.id === "settings-form") {
    const form = target.form
    setServerDraft(readServerConfigForm(form))
    clearAuthFormFeedback(form.id)
    clearAuthValidationCache()

    if (
      target instanceof HTMLInputElement &&
      (target.name === "url" ||
        target.name === "api_key" ||
        target.name === "username" ||
        target.name === "password")
    ) {
      markField(target.name, "idle")
    }
    return
  }

  if (target.form?.id !== "pipeline-form") {
    return
  }

  state.pipelineFeedback = null
  state.pipelineErrors = []

  const mappingColumn = target.dataset.mappingColumn
  if (mappingColumn) {
    const mapping = state.pipelineForm.mappings.find(item => item.csvColumn === mappingColumn)
    if (mapping) {
      mapping.datastreamId = target.value
    }
    return
  }

  updatePipelineField(target.name, target.value)
})

mainContent.addEventListener("focusout", event => {
  const target = event.target
  if (!(target instanceof HTMLInputElement) || !target.form) {
    return
  }

  if (target.form.id !== "welcome-form" && target.form.id !== "settings-form") {
    return
  }

  if (
    target.name !== "url" &&
    target.name !== "api_key" &&
    target.name !== "username" &&
    target.name !== "password"
  ) {
    return
  }

  void validateAuthField(target.form, target.name)
})

mainContent.addEventListener("click", event => {
  const target = event.target
  if (!(target instanceof HTMLElement)) {
    return
  }

  const action = target.closest<HTMLElement>("[data-action]")?.dataset.action
  const jobId = target.closest<HTMLElement>("[data-job-id]")?.dataset.jobId

  if (!action) {
    return
  }

  if (action === "retry-bootstrap") {
    void bootstrap()
    return
  }

  if (action === "toggle-auth-mode") {
    const form = target.closest<HTMLFormElement>("form")
    if (!form) {
      return
    }

    const nextServer = readServerConfigForm(form)
    const nextAuthType: AuthType = nextServer.auth_type === "apikey" ? "userpass" : "apikey"
    setServerDraft({
      ...nextServer,
      auth_type: nextAuthType,
    })
    resetAuthFieldStates(nextAuthType)

    clearAuthFormFeedback(form.id)
    clearAuthValidationCache()

    render()
    return
  }

  if (action === "change-credentials") {
    state.authDraft = {
      ...emptyServerConfig(),
      ...(state.config?.server ?? {}),
    }
    state.settingsEditMode = true
    navigate("settings")
    render()
    return
  }

  if (action === "cancel-credential-edit") {
    state.authDraft = {
      ...emptyServerConfig(),
      ...(state.config?.server ?? {}),
    }
    state.settingsEditMode = false
    render()
    return
  }

  if (action === "browse-csv") {
    void browseForCsvPath()
    return
  }

  if (action === "load-preview") {
    void loadPipelinePreview(state.pipelineForm.filePath)
    return
  }

  if (!jobId) {
    return
  }

  if (action === "run-job") {
    void handleRunJob(jobId)
    return
  }

  if (action === "toggle-job") {
    void handleToggleJob(jobId)
    return
  }

  if (action === "delete-job") {
    void handleDeleteJob(jobId)
  }
})

async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

async function handleToggleJob(jobId: string): Promise<void> {
  const job = state.jobs.find(item => item.id === jobId)
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
  const confirmed = window.confirm("Delete this pipeline?")
  if (!confirmed) {
    return
  }

  try {
    await deleteJob(jobId)
    await refreshJobs()
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

void bootstrap()
