import "./generated.css"

import {
  deleteJob,
  disableJob,
  enableJob,
  getConfig,
  getHealth,
  listJobs,
  runJob,
  testConnection,
  updateServerConfig,
  type AppConfig,
  type ConnectionState,
  type ConnectionTestResponse,
  type HealthResponse,
  type JobSummary,
  type ServerConfig,
} from "./api"
import { getRouteFromHash, navigate, routeHref, type AppRoute } from "./router"
import { formatRelativeTime, formatSchedule, shortenPath } from "./time"

type Feedback = {
  tone: "success" | "error" | "info"
  message: string
} | null

type UiState = {
  route: AppRoute
  health: HealthResponse | null
  config: AppConfig | null
  jobs: JobSummary[]
  loading: boolean
  bootstrapError: string | null
  settingsFeedback: Feedback
  welcomeFeedback: Feedback
  welcomeStep: 1 | 2
  lastConnectionState: ConnectionState | null
}

const shellElements = {
  sidebar: document.querySelector<HTMLElement>("#app-sidebar"),
  mainContent: document.querySelector<HTMLElement>("#main-content"),
  jobsLink: document.querySelector<HTMLAnchorElement>('[data-route="dashboard"]'),
  settingsLink: document.querySelector<HTMLAnchorElement>('[data-route="settings"]'),
  connectionDot: document.querySelector<HTMLElement>("#connection-status-dot"),
}

if (!shellElements.sidebar || !shellElements.mainContent || !shellElements.jobsLink || !shellElements.settingsLink || !shellElements.connectionDot) {
  throw new Error("App shell is missing required elements.")
}

const { sidebar, mainContent, jobsLink, settingsLink, connectionDot } = shellElements

const state: UiState = {
  route: getRouteFromHash(),
  health: null,
  config: null,
  jobs: [],
  loading: true,
  bootstrapError: null,
  settingsFeedback: null,
  welcomeFeedback: null,
  welcomeStep: 1,
  lastConnectionState: null,
}

const STARTUP_RETRY_ATTEMPTS = 12
const STARTUP_RETRY_DELAY_MS = 350

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

function sidebarConnectionState(): { label: string; className: string } {
  if (!state.config?.server.url || !state.config.server.api_key) {
    return { label: "HydroServer not configured", className: "status-dot bg-slate-300" }
  }

  switch (state.lastConnectionState) {
    case "connected":
      return { label: "Connected to HydroServer", className: "status-dot bg-emerald-500" }
    case "error":
      return { label: "HydroServer connection error", className: "status-dot bg-rose-500" }
    default:
      return { label: "HydroServer configured", className: "status-dot bg-sky-500" }
  }
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

function renderDashboard(): string {
  if (state.jobs.length === 0) {
    return `
      <section class="page-shell animate-fade-in">
        <header class="page-header">
          <div>
            <p class="eyebrow">Dashboard</p>
            <h1 class="page-title">Jobs</h1>
            <p class="page-copy">This loader stores its own local job definitions and pushes directly to HydroServer.</p>
          </div>
          <a class="btn-primary" href="${routeHref("jobs-new")}">Add job</a>
        </header>

        <article class="empty-panel">
          <div class="empty-icon">CSV</div>
          <h2 class="section-title">No data sources yet</h2>
          <p class="section-copy">Connect to HydroServer first, then add a watched CSV file and map its columns to datastreams.</p>
          <a class="btn-primary" href="${routeHref("jobs-new")}">Add your first job</a>
        </article>
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
          <h1 class="page-title">Jobs</h1>
          <p class="page-copy">Watch local CSV files, keep the cursor on disk, and push only new observations into HydroServer.</p>
        </div>
        <a class="btn-primary" href="${routeHref("jobs-new")}">Add job</a>
      </header>
      <div class="card-stack">${cards}</div>
    </section>
  `
}

function renderSettings(): string {
  const server = state.config?.server ?? { url: "", api_key: "" }

  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Settings</p>
          <h1 class="page-title">HydroServer connection</h1>
          <p class="page-copy">SDL now stores its own local job configuration. HydroServer is only the target system for authentication and data push.</p>
        </div>
      </header>

      <form id="settings-form" class="settings-card" autocomplete="off">
        <section class="card-section">
          <h2 class="section-title">HydroServer connection</h2>

          <label class="field">
            <span class="label">Server URL</span>
            <input class="input" type="url" name="url" value="${escapeHtml(server.url)}" placeholder="https://hydroserver.example.com" />
          </label>

          <label class="field">
            <span class="label">API key</span>
            <input class="input" type="password" name="api_key" value="${escapeHtml(server.api_key)}" placeholder="hs_live_..." />
          </label>

          <div class="button-row">
            <button class="btn-ghost" type="button" data-action="test-connection">Test connection</button>
            <button class="btn-primary" type="submit">Save</button>
          </div>

          ${feedbackMarkup(state.settingsFeedback)}
        </section>

        <section class="card-section muted-section">
          <h2 class="section-title">Preferences</h2>
          <p class="section-copy">Launch-at-login and tray controls arrive in the desktop integration phase.</p>
        </section>

        <section class="card-section muted-section">
          <h2 class="section-title">About</h2>
          <p class="section-copy">SDL version ${escapeHtml(state.health?.version ?? "0.1.0")}</p>
          <p class="section-copy">HydroServer Streaming Data Loader</p>
        </section>
      </form>
    </section>
  `
}

function renderWelcome(): string {
  const server = state.config?.server ?? { url: "", api_key: "" }

  if (state.welcomeStep === 2) {
    return `
      <section class="welcome-shell animate-fade-in">
        <div class="welcome-card">
          <p class="eyebrow">Connected</p>
          <h1 class="page-title">HydroServer is ready</h1>
          <p class="page-copy">The next step in the implementation order is the job editor and CSV preview. Your HydroServer credentials are already saved locally.</p>
          <div class="button-row">
            <a class="btn-primary" href="${routeHref("dashboard")}">Continue to dashboard</a>
            <a class="btn-ghost" href="${routeHref("settings")}">Review settings</a>
          </div>
        </div>
      </section>
    `
  }

  return `
    <section class="welcome-shell animate-fade-in">
      <form id="welcome-form" class="welcome-card" autocomplete="off">
        <p class="eyebrow">Welcome</p>
        <h1 class="page-title">Connect to your HydroServer instance</h1>
        <p class="page-copy">SDL now manages its own local job definitions, then authenticates with HydroServer only when it needs to discover datastreams or push new observations.</p>

        <label class="field">
          <span class="label">Server URL</span>
          <input class="input" type="url" name="url" value="${escapeHtml(server.url)}" placeholder="https://hydroserver.example.com" />
        </label>

        <label class="field">
          <span class="label">API key</span>
          <input class="input" type="password" name="api_key" value="${escapeHtml(server.api_key)}" placeholder="hs_live_..." />
        </label>

        <div class="button-row">
          <button class="btn-primary" type="submit">Connect</button>
          <a class="btn-ghost" href="${routeHref("settings")}">Open settings</a>
        </div>

        ${feedbackMarkup(state.welcomeFeedback)}
      </form>
    </section>
  `
}

function renderJobsPlaceholder(): string {
  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Next phase</p>
          <h1 class="page-title">Job editor</h1>
          <p class="page-copy">The backend now stores local SDL job definitions in <code>config.json</code>. The next implementation step is the real editor, CSV preview, and column-to-datastream mapping workflow.</p>
        </div>
      </header>
      <article class="empty-panel">
        <div class="empty-icon">1</div>
        <h2 class="section-title">Foundation is in place</h2>
        <p class="section-copy">Use the Settings page to connect to HydroServer, then the next pass will add the actual file picker, preview surface, and mapping UI.</p>
        <a class="btn-ghost" href="${routeHref("dashboard")}">Back to dashboard</a>
      </article>
    </section>
  `
}

function renderFatalError(): string {
  return `
    <section class="welcome-shell animate-fade-in">
      <div class="welcome-card">
        <p class="eyebrow">Sidecar error</p>
        <h1 class="page-title">The background process is unavailable</h1>
        <p class="page-copy">${escapeHtml(state.bootstrapError ?? "SDL could not reach the local sidecar.")}</p>
        <button class="btn-primary" type="button" data-action="retry-bootstrap">Retry</button>
      </div>
    </section>
  `
}

function render(): void {
  state.route = getRouteFromHash()

  if (!state.loading && !state.bootstrapError && !state.config?.server.url && state.route !== "settings" && state.route !== "welcome") {
    navigate("welcome")
    state.route = "welcome"
  }

  const currentRoute = getRouteFromHash()
  const showSidebar = currentRoute !== "welcome" && !state.bootstrapError
  sidebar.classList.toggle("hidden", !showSidebar)

  jobsLink.className = currentRoute === "dashboard" ? "nav-item nav-item-active" : "nav-item"
  settingsLink.className = currentRoute === "settings" ? "nav-item nav-item-active" : "nav-item"

  const connectionState = sidebarConnectionState()
  connectionDot.className = connectionState.className
  connectionDot.title = connectionState.label

  if (state.loading) {
    mainContent.innerHTML = `
      <section class="welcome-shell">
        <div class="welcome-card">
          <p class="eyebrow">Starting SDL</p>
          <h1 class="page-title">Loading local configuration</h1>
          <p class="page-copy">Connecting the browser preview to the FastAPI sidecar.</p>
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
    mainContent.innerHTML = renderJobsPlaceholder()
    return
  }

  mainContent.innerHTML = renderDashboard()
}

function readServerForm(form: HTMLFormElement): ServerConfig {
  const data = new FormData(form)
  return {
    url: String(data.get("url") ?? "").trim(),
    api_key: String(data.get("api_key") ?? "").trim(),
  }
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

  throw lastError instanceof Error ? lastError : new Error("Failed to load SDL.")
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
  } catch (error) {
    state.bootstrapError = error instanceof Error ? error.message : "Failed to load SDL."
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
    // Keep the existing dashboard state if polling fails.
  }
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

  if (target.id === "settings-form") {
    event.preventDefault()
    void saveSettings(target)
  }

  if (target.id === "welcome-form") {
    event.preventDefault()
    void connectWelcome(target)
  }
})

mainContent.addEventListener("input", event => {
  const target = event.target
  if (!(target instanceof HTMLInputElement)) {
    return
  }

  if (target.form?.id === "settings-form") {
    state.settingsFeedback = null
  }

  if (target.form?.id === "welcome-form") {
    state.welcomeFeedback = null
  }
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

  if (action === "test-connection") {
    const form = document.querySelector<HTMLFormElement>("#settings-form")
    if (form) {
      void testSettingsConnection(form)
    }
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

async function saveSettings(form: HTMLFormElement): Promise<void> {
  const payload = readServerForm(form)

  try {
    state.config = await updateServerConfig(payload)
    state.settingsFeedback = { tone: "success", message: "Settings saved." }
    state.lastConnectionState = "configured"
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Failed to save settings.",
    }
  }

  render()
}

async function testSettingsConnection(form: HTMLFormElement): Promise<void> {
  const payload = readServerForm(form)

  try {
    const result = await testConnection(payload)
    applyConnectionFeedback(result, "settings")
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't test the HydroServer connection.",
    }
    state.lastConnectionState = "error"
  }

  render()
}

async function connectWelcome(form: HTMLFormElement): Promise<void> {
  const payload = readServerForm(form)

  try {
    const result = await testConnection(payload)
    if (!result.ok) {
      applyConnectionFeedback(result, "welcome")
      render()
      return
    }

    state.config = await updateServerConfig(payload)
    state.lastConnectionState = "connected"
    state.welcomeFeedback = { tone: "success", message: result.message }
    state.welcomeStep = 2
    render()
  } catch (error) {
    state.welcomeFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't save the HydroServer connection.",
    }
    state.lastConnectionState = "error"
    render()
  }
}

function applyConnectionFeedback(result: ConnectionTestResponse, context: "settings" | "welcome"): void {
  const feedback: Feedback = {
    tone: result.ok ? "success" : "error",
    message: result.message,
  }

  if (context === "settings") {
    state.settingsFeedback = feedback
  } else {
    state.welcomeFeedback = feedback
  }

  state.lastConnectionState = result.state
}

async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId)
    await refreshJobs()
  } catch {
    // Dashboard cards already show persistent error state from the sidecar.
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
    // Ignore and keep the current UI state.
  }
}

async function handleDeleteJob(jobId: string): Promise<void> {
  const confirmed = window.confirm("Delete this job?")
  if (!confirmed) {
    return
  }

  try {
    await deleteJob(jobId)
    await refreshJobs()
  } catch {
    // Ignore and keep the current UI state.
  }
}

void bootstrap()
