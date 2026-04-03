import "./generated.css";
import appIconUrl from "../icons/icon-color.svg";

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
} from "./api";
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
} from "./auth-submit";
import { getRouteFromHash, navigate, routeHref, type AppRoute } from "./router";
import { formatRelativeTime, formatSchedule, shortenPath } from "./time";

const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key";
const APP_NAME = "HydroServer Streaming Data Loader";
const STARTUP_RETRY_ATTEMPTS = 12;
const STARTUP_RETRY_DELAY_MS = 350;

type PipelineMappingDraft = {
  csvColumn: string;
  datastreamId: string;
};

type PipelineFormState = {
  name: string;
  filePath: string;
  scheduleMinutes: number;
  headerRow: number;
  dataStartRow: number;
  delimiter: string;
  timestampColumn: string;
  timestampFormat: string;
  timezone: string;
  mappings: PipelineMappingDraft[];
};

type PreviewSelectionTarget =
  | "header-row"
  | "data-start-row"
  | "timestamp-column"
  | null;

type UiState = {
  route: AppRoute;
  health: HealthResponse | null;
  config: AppConfig | null;
  jobs: JobSummary[];
  datastreams: DatastreamSummary[];
  connectionSummary: ConnectionTestResponse | null;
  loading: boolean;
  bootstrapError: string | null;
  settingsFeedback: Feedback;
  welcomeFeedback: Feedback;
  pipelineFeedback: Feedback;
  lastConnectionState: ConnectionState | null;
  settingsEditMode: boolean;
  pipelineForm: PipelineFormState;
  pipelinePreview: CsvPreviewResponse | null;
  pipelineErrors: string[];
  datastreamsError: string | null;
  authDraft: ServerConfig;
  authFieldStates: Record<AuthFieldName, FieldValidationState>;
  authSubmitting: boolean;
  lastAuthValidationServer: ServerConfig | null;
  lastAuthValidationResult: ConnectionTestResponse | null;
  pipelineSelectionTarget: PreviewSelectionTarget;
};

const shellElements = {
  sidebar: document.querySelector<HTMLElement>("#app-sidebar"),
  mainContent: document.querySelector<HTMLElement>("#main-content"),
  jobsLink: document.querySelector<HTMLAnchorElement>(
    '[data-route="dashboard"]'
  ),
  settingsLink: document.querySelector<HTMLAnchorElement>(
    '[data-route="settings"]'
  ),
  connectionDot: document.querySelector<HTMLElement>("#connection-status-dot"),
};

if (
  !shellElements.sidebar ||
  !shellElements.mainContent ||
  !shellElements.jobsLink ||
  !shellElements.settingsLink ||
  !shellElements.connectionDot
) {
  throw new Error("App shell is missing required elements.");
}

const { sidebar, mainContent, jobsLink, settingsLink, connectionDot } =
  shellElements;

let lastRenderedMarkup = "";

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
  };
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
  authFieldStates: createAuthFieldStates(),
  authSubmitting: false,
  lastAuthValidationServer: null,
  lastAuthValidationResult: null,
  pipelineSelectionTarget: null,
};

function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
  };
}

window.setInterval(() => {
  void refreshJobs();
}, 30_000);

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function feedbackMarkup(feedback: Feedback): string {
  if (!feedback) {
    return "";
  }

  const toneClass =
    feedback.tone === "success"
      ? "notice-success"
      : feedback.tone === "error"
      ? "notice-error"
      : "notice-info";

  return `<div class="${toneClass}">${escapeHtml(feedback.message)}</div>`;
}

function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments.at(-1) ?? path;
}

function parseDelimitedLine(line: string, delimiter: string): string[] {
  if (!delimiter) {
    return [line];
  }

  const cells: string[] = [];
  let current = "";
  let inQuotes = false;

  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];

    if (character === '"') {
      if (inQuotes && line[index + 1] === '"') {
        current += '"';
        index += 1;
      } else {
        inQuotes = !inQuotes;
      }
      continue;
    }

    if (!inQuotes && line.startsWith(delimiter, index)) {
      cells.push(current);
      current = "";
      index += delimiter.length - 1;
      continue;
    }

    current += character;
  }

  cells.push(current);
  return cells;
}

function normalizePreviewHeaderName(value: string, index: number): string {
  const cleaned = value.trim();
  return cleaned || `Column ${index + 1}`;
}

function parsedPreviewRows(): string[][] {
  if (!state.pipelinePreview) {
    return [];
  }

  return state.pipelinePreview.raw_lines.map((line) =>
    parseDelimitedLine(line, state.pipelineForm.delimiter)
  );
}

function connected(): boolean {
  return (
    state.connectionSummary?.ok === true &&
    state.lastConnectionState === "connected"
  );
}

function currentServerConfig(): ServerConfig {
  return state.authDraft;
}

function resetStateAuthFieldStates(authType: AuthType): void {
  resetAuthFieldStates(state.authFieldStates, authType);
}

function serverConfigured(server: ServerConfig | null | undefined): boolean {
  if (!server?.url.trim()) {
    return false;
  }

  if (server.auth_type === "userpass") {
    return Boolean(server.username.trim() && server.password.trim());
  }

  return Boolean(server.api_key.trim());
}

function readServerConfigForm(
  form: HTMLFormElement,
  base: ServerConfig = currentServerConfig()
): ServerConfig {
  const data = new FormData(form);
  const authType = data.get("auth_type") === "userpass" ? "userpass" : "apikey";

  return {
    auth_type: authType,
    url: String(data.get("url") ?? "").trim(),
    api_key:
      authType === "apikey"
        ? String(data.get("api_key") ?? "").trim()
        : base.api_key,
    username:
      authType === "userpass"
        ? String(data.get("username") ?? "").trim()
        : base.username,
    password:
      authType === "userpass"
        ? String(data.get("password") ?? "").trim()
        : base.password,
  };
}

function setServerDraft(server: ServerConfig): void {
  state.authDraft = { ...server };
}

function markField(
  field: AuthFieldName,
  nextState: FieldValidationState["state"],
  message: string | null = null
): void {
  state.authFieldStates[field] = { state: nextState, message };
}

function authFieldErrorMarkup(field: AuthFieldName): string {
  const fieldState = state.authFieldStates[field];
  if (fieldState.state !== "invalid" || !fieldState.message) {
    return "";
  }

  return `<p class="field-error">${escapeHtml(fieldState.message)}</p>`;
}

function renderAuthInputField(params: {
  label: string;
  name: AuthFieldName;
  type: "url" | "text" | "password";
  value: string;
  placeholder: string;
  helpText?: string;
  labelAction?: string;
}): string {
  const { label, name, type, value, placeholder, helpText, labelAction } =
    params;

  return `
    <label class="field">
      <span class="field-label-row">
        <span class="label">${escapeHtml(label)}</span>
        ${labelAction ?? ""}
      </span>
      <input class="input" type="${type}" name="${name}" value="${escapeHtml(
        value
      )}" placeholder="${escapeHtml(placeholder)}" />
      ${helpText ? `<p class="field-hint">${escapeHtml(helpText)}</p>` : ""}
      ${authFieldErrorMarkup(name)}
    </label>
  `;
}

function clearAuthFormFeedback(formId: string): void {
  state[fieldFormFeedbackTarget(formId)] = null;
}

function clearAuthValidationCache(): void {
  state.lastAuthValidationServer = null;
  state.lastAuthValidationResult = null;
}

function previewHeaders(): string[] {
  const rows = parsedPreviewRows();
  const headerRow = rows[state.pipelineForm.headerRow - 1] ?? [];
  return headerRow.map((cell, index) =>
    normalizePreviewHeaderName(cell, index)
  );
}

function pipelineMappingsByColumn(): Map<string, string> {
  return new Map(
    state.pipelineForm.mappings.map((mapping) => [
      mapping.csvColumn,
      mapping.datastreamId,
    ])
  );
}

function previewColumnClass(columnName: string): string {
  if (columnName === state.pipelineForm.timestampColumn) {
    return "preview-col-timestamp";
  }

  const mapped = state.pipelineForm.mappings.find(
    (mapping) => mapping.csvColumn === columnName && mapping.datastreamId
  );
  return mapped ? "preview-col-mapped" : "";
}

function previewPickerButtonClass(
  target: Exclude<PreviewSelectionTarget, null>
): string {
  const active = state.pipelineSelectionTarget === target;
  const toneClass =
    target === "header-row"
      ? "field-picker-header"
      : target === "data-start-row"
      ? "field-picker-data"
      : "field-picker-timestamp";

  return active
    ? `field-picker field-picker-active ${toneClass}`
    : "field-picker";
}

function previewFieldClass(
  target: Exclude<PreviewSelectionTarget, null>
): string {
  const active = state.pipelineSelectionTarget === target;
  const toneClass =
    target === "header-row"
      ? "preview-bound-field-header"
      : target === "data-start-row"
      ? "preview-bound-field-data"
      : "preview-bound-field-timestamp";

  return active
    ? `field preview-bound-field preview-bound-field-active ${toneClass}`
    : "field preview-bound-field";
}

function previewGuidanceText(): string {
  if (state.pipelineSelectionTarget === "header-row") {
    return "Click a raw line to set the header row.";
  }

  if (state.pipelineSelectionTarget === "data-start-row") {
    return "Click the first data line in the raw preview.";
  }

  if (state.pipelineSelectionTarget === "timestamp-column") {
    return "Click a column header to set the timestamp column.";
  }

  return "Use the picker controls on the left, or click a column header to set the timestamp column directly.";
}

function syncPipelineSelectionsWithPreview(): void {
  const headers = previewHeaders();

  if (headers.length === 0) {
    state.pipelineForm.mappings = [];
    return;
  }

  const preferredTimestamp =
    headers.find((header) => header.toLowerCase().includes("time")) ??
    headers[0];

  state.pipelineForm.timestampColumn = headers.includes(
    state.pipelineForm.timestampColumn
  )
    ? state.pipelineForm.timestampColumn
    : preferredTimestamp;

  initializeMappings(headers);
}

function initializeMappings(headers: string[]): void {
  const existing = pipelineMappingsByColumn();
  state.pipelineForm.mappings = headers
    .filter((header) => header !== state.pipelineForm.timestampColumn)
    .map((header) => ({
      csvColumn: header,
      datastreamId: existing.get(header) ?? "",
    }));
}

function applyPreview(path: string, preview: CsvPreviewResponse): void {
  state.pipelinePreview = preview;
  state.pipelineForm.filePath = path;
  state.pipelineForm.headerRow =
    preview.detected_header_row ?? state.pipelineForm.headerRow;
  state.pipelineForm.dataStartRow =
    preview.detected_data_start_row ?? state.pipelineForm.dataStartRow;
  state.pipelineForm.delimiter =
    preview.detected_delimiter || state.pipelineForm.delimiter;
  state.pipelineSelectionTarget = null;

  if (!state.pipelineForm.name.trim()) {
    const inferred = basename(path).replace(/\.[^.]+$/, "");
    state.pipelineForm.name = inferred;
  }

  syncPipelineSelectionsWithPreview();
}

function updateHeaderRowFromPreview(lineNumber: number): void {
  state.pipelineForm.headerRow = lineNumber;
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1;
  }
  syncPipelineSelectionsWithPreview();
}

function updateDataStartRowFromPreview(lineNumber: number): void {
  state.pipelineForm.dataStartRow = Math.max(2, lineNumber);
  if (state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow) {
    state.pipelineForm.headerRow = state.pipelineForm.dataStartRow - 1;
  }
  syncPipelineSelectionsWithPreview();
}

function applyPreviewLineSelection(lineNumber: number): void {
  if (state.pipelineSelectionTarget === "header-row") {
    updateHeaderRowFromPreview(lineNumber);
    state.pipelineSelectionTarget = null;
    render();
    return;
  }

  if (state.pipelineSelectionTarget === "data-start-row") {
    updateDataStartRowFromPreview(lineNumber);
    state.pipelineSelectionTarget = null;
    render();
  }
}

function applyPreviewColumnSelection(columnName: string): void {
  if (
    state.pipelineSelectionTarget &&
    state.pipelineSelectionTarget !== "timestamp-column"
  ) {
    return;
  }

  state.pipelineForm.timestampColumn = columnName;
  initializeMappings(previewHeaders());
  state.pipelineSelectionTarget = null;
  render();
}

function onboardingRoute(route: AppRoute): boolean {
  return route === "welcome" || (route === "jobs-new" && state.jobs.length === 0);
}

function connectionIndicator(): { label: string; className: string } {
  if (!serverConfigured(state.config?.server)) {
    return {
      label: "HydroServer not configured",
      className: "status-dot bg-slate-300",
    };
  }

  if (connected()) {
    return {
      label: "Connected to HydroServer",
      className: "status-dot bg-emerald-500",
    };
  }

  if (state.lastConnectionState === "error") {
    return {
      label: "HydroServer authentication error",
      className: "status-dot bg-rose-500",
    };
  }

  return {
    label: "HydroServer configured",
    className: "status-dot bg-sky-500",
  };
}

function statusPill(job: JobSummary): string {
  const classes: Record<JobSummary["status"], string> = {
    healthy: "pill-success",
    warning: "pill-warning",
    error: "pill-danger",
    disabled: "pill-muted",
    pending: "pill-info",
    running: "pill-info",
  };

  return `<span class="${classes[job.status]}">${escapeHtml(
    job.status_message
  )}</span>`;
}

function renderConnectedCard(showActions: boolean): string {
  if (!connected() || !state.connectionSummary) {
    return "";
  }

  const datastreamText =
    state.connectionSummary.datastream_count === 1
      ? "1 datastream available"
      : `${state.connectionSummary.datastream_count} datastreams available`;

  return `
    <article class="summary-card">
      <div class="summary-card-copy">
        <p class="eyebrow">Authenticated</p>
        <h2 class="section-title">${escapeHtml(
          state.connectionSummary.instance_name ?? "HydroServer"
        )}</h2>
        <p class="section-copy">${escapeHtml(
          state.connectionSummary.message
        )}</p>
        <div class="summary-inline">
          <span class="pill-success">Connected</span>
          <span class="summary-meta">${escapeHtml(datastreamText)}</span>
        </div>
      </div>
      ${
        showActions
          ? `
        <div class="button-row">
          <button class="btn-danger" type="button" data-action="disconnect">Disconnect</button>
          <button class="btn-ghost" type="button" data-action="change-credentials">Change credentials</button>
          ${
            state.jobs.length === 0
              ? `<a class="btn-primary" href="${routeHref(
                  "jobs-new"
                )}">Create first pipeline</a>`
              : ""
          }
        </div>
      `
          : ""
      }
    </article>
  `;
}

function renderAuthForm(
  formId: "welcome-form" | "settings-form",
  submitLabel: string,
  secondaryAction: string
): string {
  const server = currentServerConfig();
  const usingUserPass = server.auth_type === "userpass";
  const authToggleLabel = usingUserPass
    ? "Connect with an API key"
    : "Connect with username and password";
  const submitDisabled = state.authSubmitting ? "disabled" : "";
  const submitLabelText = state.authSubmitting ? "Connecting..." : submitLabel;

  return `
    <form id="${formId}" class="auth-card" autocomplete="off">
      <section class="card-section">
        <div class="auth-header">
          <img class="auth-app-icon" src="${appIconUrl}" alt="HydroServer Streaming Data Loader icon" />
          <h1 class="page-title">Connect to HydroServer</h1>
        </div>
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
                placeholder:
                  "KaTz74swGqHn__I2VY6ceIzrIxC04oDhUrLLgBTH9ACxYIunmkrdmqk",
                labelAction: `<a class="label-link" href="${API_KEY_DOCS_URL}" target="_blank" rel="noreferrer">How to create an API key &rarr;</a>`,
              })}
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
          <button class="btn-primary" type="submit" ${submitDisabled}>${escapeHtml(
    submitLabelText
  )}</button>
        </div>
      </section>
    </form>
  `;
}

function renderWelcome(): string {
  return `
    <section class="welcome-shell">
      ${renderAuthForm(
        "welcome-form",
        "Connect to HydroServer",
        ""
      )}
    </section>
  `;
}

function renderSettings(): string {
  const showForm = !connected() || state.settingsEditMode;

  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Settings</p>
          <h1 class="page-title">HydroServer connection</h1>
          <p class="page-copy">After ${APP_NAME} is connected, this form stays out of the way. You can return here any time to rotate credentials or verify access again.</p>
        </div>
      </header>

      ${
        showForm
          ? renderAuthForm(
              "settings-form",
              "Save and verify",
              connected()
                ? '<button class="btn-ghost" type="button" data-action="cancel-credential-edit">Cancel</button>'
                : ""
            )
          : renderConnectedCard(true)
      }
    </section>
  `;
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
          <a class="btn-primary" href="${routeHref(
            "jobs-new"
          )}">Create first pipeline</a>
        </header>
      </section>
    `;
  }

  const cards = state.jobs
    .map((job) => {
      const lastLine = job.last_error
        ? `Failed ${formatRelativeTime(job.last_run_at)}`
        : `Last pushed ${formatRelativeTime(job.last_pushed_timestamp)}`;

      return `
        <article class="job-card animate-fade-in">
          <div class="job-card-top">
            <div>
              <div class="job-card-title-row">
                <span class="status-dot ${
                  job.status === "error"
                    ? "bg-rose-500"
                    : job.status === "warning"
                    ? "bg-amber-500"
                    : job.status === "disabled"
                    ? "bg-slate-300"
                    : "bg-emerald-500"
                }"></span>
                <h2 class="section-title">${escapeHtml(job.name)}</h2>
              </div>
              <p class="section-copy">${escapeHtml(
                shortenPath(job.file_path)
              )}</p>
              <p class="job-meta ${
                job.status === "error" ? "text-rose-600" : ""
              }">
                ${escapeHtml(lastLine)} · ${escapeHtml(
        formatSchedule(job.schedule_minutes)
      )}
              </p>
            </div>
            ${statusPill(job)}
          </div>

          <div class="job-card-actions">
            <button class="btn-ghost" data-action="run-job" data-job-id="${
              job.id
            }">Run now</button>
            <button class="btn-ghost" data-action="toggle-job" data-job-id="${
              job.id
            }">
              ${job.enabled ? "Disable" : "Enable"}
            </button>
            <button class="btn-danger" data-action="delete-job" data-job-id="${
              job.id
            }">Delete</button>
          </div>
        </article>
      `;
    })
    .join("");

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
  `;
}

function renderPipelinePreview(): string {
  if (!state.pipelinePreview) {
    return `
      <article class="preview-card">
        <div class="preview-placeholder">
          <div class="empty-icon">CSV</div>
          <h2 class="section-title">Preview a source file</h2>
          <p class="section-copy">Choose a CSV file path, then load the preview to inspect the first 50 lines and map the source structure into HydroServer.</p>
        </div>
      </article>
    `;
  }

  const headers = previewHeaders();
  const parsedRows = parsedPreviewRows()
    .slice(Math.max(state.pipelineForm.dataStartRow - 1, 0))
    .map((row, index) => ({
      lineNumber: state.pipelineForm.dataStartRow + index,
      row,
    }))
    .filter(({ row }) => row.some((cell) => cell.trim()))
    .slice(0, 8);
  const rawRows = state.pipelinePreview.raw_lines
    .map((line, index) => {
      const lineNumber = index + 1;
      const rowClass =
        lineNumber === state.pipelineForm.headerRow
          ? "preview-raw-line preview-raw-line-header"
          : lineNumber === state.pipelineForm.dataStartRow
          ? "preview-raw-line preview-raw-line-data"
          : "preview-raw-line";

      const rowTag =
        lineNumber === state.pipelineForm.headerRow
          ? '<span class="preview-row-tag preview-row-tag-header">Header</span>'
          : lineNumber === state.pipelineForm.dataStartRow
          ? '<span class="preview-row-tag preview-row-tag-data">Data start</span>'
          : "";

      return `
        <button class="${rowClass}" type="button" data-action="pick-preview-line" data-preview-line="${lineNumber}">
          <span class="preview-line-number-shell">
            <span class="preview-line-number">${lineNumber}</span>
            ${rowTag}
          </span>
          <code>${escapeHtml(line)}</code>
        </button>
      `;
    })
    .join("");

  const headerCells = headers
    .map(
      (header) =>
        `<th class="preview-cell ${previewColumnClass(header)}">
          <button class="preview-header-button" type="button" data-action="pick-preview-column" data-preview-column="${escapeHtml(
            header
          )}">
            ${escapeHtml(header)}
          </button>
        </th>`
    )
    .join("");

  const tableRows = parsedRows
    .map(
      ({ lineNumber, row }) => `
        <tr>
          <td class="preview-cell preview-cell-line-number">${lineNumber}</td>
          ${row
            .map((cell, index) => {
              const columnName = headers[index] ?? "";
              return `<td class="preview-cell ${previewColumnClass(
                columnName
              )}">${escapeHtml(cell)}</td>`;
            })
            .join("")}
        </tr>
      `
    )
    .join("");

  return `
    <article class="preview-card">
      <div class="preview-header">
        <div>
          <p class="eyebrow">Preview</p>
          <h2 class="section-title">${escapeHtml(
            basename(state.pipelineForm.filePath)
          )}</h2>
          <p class="preview-guidance">${escapeHtml(previewGuidanceText())}</p>
        </div>
        <div class="preview-summary">
          <span class="pill-info">Header row ${
            state.pipelineForm.headerRow
          }</span>
          <span class="pill-info">Data starts ${
            state.pipelineForm.dataStartRow
          }</span>
          <span class="pill-info">${escapeHtml(
            state.pipelinePreview.encoding
          )}</span>
        </div>
      </div>

      <div class="preview-raw">${rawRows}</div>

      <div class="preview-table-shell">
        <table class="preview-table">
          <thead>
            <tr>
              <th class="preview-cell preview-cell-line-number">Line</th>
              ${headerCells}
            </tr>
          </thead>
          <tbody>
            ${tableRows}
          </tbody>
        </table>
      </div>

      <footer class="preview-footer">
        Showing the first ${Math.min(
          state.pipelinePreview.total_lines,
          state.pipelinePreview.raw_lines.length
        )} lines of ${state.pipelinePreview.total_lines}
      </footer>
    </article>
  `;
}

function renderPipelineMappings(): string {
  const availableMappings = state.pipelineForm.mappings;

  if (!state.pipelinePreview || availableMappings.length === 0) {
    return `
      <div class="pipeline-subcard">
        <h3 class="section-title">Column mappings</h3>
        <p class="section-copy">Load a CSV preview first so HydroServer Streaming Data Loader can list the available source columns.</p>
      </div>
    `;
  }

  const rows = availableMappings
    .map((mapping) => {
      const options = [
        `<option value="">Not mapped</option>`,
        ...state.datastreams.map(
          (datastream) =>
            `<option value="${escapeHtml(datastream.id)}" ${
              datastream.id === mapping.datastreamId ? "selected" : ""
            }>${escapeHtml(datastream.name)}</option>`
        ),
      ].join("");

      return `
        <div class="mapping-row">
          <div>
            <p class="mapping-source">${escapeHtml(mapping.csvColumn)}</p>
            <p class="mapping-help">Source column</p>
          </div>
          <select class="input" data-mapping-column="${escapeHtml(
            mapping.csvColumn
          )}">
            ${options}
          </select>
        </div>
      `;
    })
    .join("");

  return `
    <div class="pipeline-subcard">
      <h3 class="section-title">Column mappings</h3>
      <p class="section-copy">Map each source column to a HydroServer datastream. Leave any unused source columns as “Not mapped.”</p>
      <div class="mapping-grid">${rows}</div>
    </div>
  `;
}

function renderPipelineEditor(): string {
  const firstRunOnboarding = state.jobs.length === 0;
  const shellClass = firstRunOnboarding
    ? "page-shell onboarding-shell animate-fade-in"
    : "page-shell animate-fade-in";

  if (!connected()) {
    return renderWelcome();
  }

  if (state.datastreamsError) {
    return `
      <section class="${shellClass}">
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
    `;
  }

  if (state.datastreams.length === 0) {
    return `
      <section class="${shellClass}">
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
    `;
  }

  const timestampOptions = previewHeaders()
    .map(
      (header) =>
        `<option value="${escapeHtml(header)}" ${
          header === state.pipelineForm.timestampColumn ? "selected" : ""
        }>${escapeHtml(header)}</option>`
    )
    .join("");

  const pipelineErrorMarkup =
    state.pipelineErrors.length > 0
      ? `
        <div class="validation-panel">
          <h3 class="section-title">Fix these issues before saving</h3>
          <ul class="validation-list">
            ${state.pipelineErrors
              .map((error) => `<li>${escapeHtml(error)}</li>`)
              .join("")}
          </ul>
        </div>
      `
      : "";

  return `
    <section class="${shellClass}">
      <header class="page-header">
        <div>
          <p class="eyebrow">${
            firstRunOnboarding ? "Step 2 of 2" : "Create first pipeline"
          }</p>
          <h1 class="page-title">Connect a CSV source to HydroServer</h1>
          <p class="page-copy">Choose the CSV file you want ${APP_NAME} to watch, preview the first 50 lines, then click the structure on the right to fill the setup form on the left.</p>
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
                    (minutes) =>
                      `<option value="${minutes}" ${
                        state.pipelineForm.scheduleMinutes === minutes
                          ? "selected"
                          : ""
                      }>Every ${formatSchedule(minutes).replace(
                        "Every ",
                        ""
                      )}</option>`
                  )
                  .join("")}
              </select>
            </label>
          </div>

          <div class="pipeline-subcard">
            <h2 class="section-title">File structure</h2>

            <div class="split-fields">
              <div class="${previewFieldClass("header-row")}">
                <div class="field-label-row">
                  <label class="label" for="pipeline-header-row">Header row</label>
                  <button
                    class="${previewPickerButtonClass("header-row")}"
                    type="button"
                    data-action="toggle-preview-picker"
                    data-picker-target="header-row"
                  >
                    ${
                      state.pipelineSelectionTarget === "header-row"
                        ? "Picking in preview"
                        : "Pick from preview"
                    }
                  </button>
                </div>
                <input id="pipeline-header-row" class="input" type="number" min="1" name="header_row" value="${
                  state.pipelineForm.headerRow
                }" />
                <span class="field-hint">Click the line that contains the CSV column names.</span>
              </div>

              <div class="${previewFieldClass("data-start-row")}">
                <div class="field-label-row">
                  <label class="label" for="pipeline-data-start-row">Data start row</label>
                  <button
                    class="${previewPickerButtonClass("data-start-row")}"
                    type="button"
                    data-action="toggle-preview-picker"
                    data-picker-target="data-start-row"
                  >
                    ${
                      state.pipelineSelectionTarget === "data-start-row"
                        ? "Picking in preview"
                        : "Pick from preview"
                    }
                  </button>
                </div>
                <input id="pipeline-data-start-row" class="input" type="number" min="1" name="data_start_row" value="${
                  state.pipelineForm.dataStartRow
                }" />
                <span class="field-hint">Choose the first line that contains actual observation values.</span>
              </div>
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

            <div class="${previewFieldClass("timestamp-column")}">
              <div class="field-label-row">
                <label class="label" for="pipeline-timestamp-column">Timestamp column</label>
                <button
                  class="${previewPickerButtonClass("timestamp-column")}"
                  type="button"
                  data-action="toggle-preview-picker"
                  data-picker-target="timestamp-column"
                >
                  ${
                    state.pipelineSelectionTarget === "timestamp-column"
                      ? "Picking in preview"
                      : "Pick from preview"
                  }
                </button>
              </div>
              ${
                previewHeaders().length > 0
                  ? `<select id="pipeline-timestamp-column" class="input" name="timestamp_column">${timestampOptions}</select>`
                  : `<input id="pipeline-timestamp-column" class="input" type="text" name="timestamp_column" value="${escapeHtml(
                      state.pipelineForm.timestampColumn
                    )}" placeholder="Timestamp" />`
              }
              <span class="field-hint">Click the matching header in the preview table to bind it.</span>
            </div>

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
  `;
}

function renderFatalError(): string {
  return `
    <section class="welcome-shell">
      <div class="welcome-card">
        <p class="eyebrow">Sidecar error</p>
        <h1 class="page-title">The background process is unavailable</h1>
        <p class="page-copy">${escapeHtml(
          state.bootstrapError ??
            `${APP_NAME} could not reach the local background service.`
        )}</p>
        <button class="btn-primary" type="button" data-action="retry-bootstrap">Retry</button>
      </div>
    </section>
  `;
}

function render(): void {
  state.route = getRouteFromHash();

  let currentRoute = getRouteFromHash();

  if (!state.loading && !state.bootstrapError) {
    if (
      !connected() &&
      currentRoute !== "settings" &&
      currentRoute !== "welcome"
    ) {
      navigate("welcome");
      currentRoute = "welcome";
    } else if (
      connected() &&
      state.jobs.length === 0 &&
      (currentRoute === "dashboard" || currentRoute === "welcome")
    ) {
      navigate("jobs-new");
      currentRoute = "jobs-new";
    }
  }

  const inOnboardingRoute = onboardingRoute(currentRoute);
  const showSidebar = !inOnboardingRoute && !state.bootstrapError;
  const useWelcomeSurface = Boolean(
    state.loading || state.bootstrapError || inOnboardingRoute
  );
  sidebar.classList.toggle("hidden", !showSidebar);
  mainContent.classList.toggle("main-content-welcome", useWelcomeSurface);
  document.body.classList.toggle("app-surface-welcome", useWelcomeSurface);

  jobsLink.className =
    currentRoute === "dashboard" ? "nav-item nav-item-active" : "nav-item";
  settingsLink.className =
    currentRoute === "settings" ? "nav-item nav-item-active" : "nav-item";

  const status = connectionIndicator();
  connectionDot.className = status.className;
  connectionDot.title = status.label;

  let nextMarkup = "";

  if (state.loading) {
    nextMarkup = `
      <section class="loading-shell" aria-label="Loading">
        <div class="loading-spinner" aria-hidden="true"></div>
      </section>
    `;
  } else if (state.bootstrapError) {
    nextMarkup = renderFatalError();
  } else if (currentRoute === "settings") {
    nextMarkup = renderSettings();
  } else if (currentRoute === "welcome") {
    nextMarkup = renderWelcome();
  } else if (currentRoute === "jobs-new") {
    nextMarkup = renderPipelineEditor();
  } else {
    nextMarkup = renderDashboard();
  }

  if (nextMarkup !== lastRenderedMarkup) {
    mainContent.innerHTML = nextMarkup;
    lastRenderedMarkup = nextMarkup;
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isTransientBootstrapError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }

  const message = error.message.toLowerCase();
  return (
    message.includes("failed to fetch") ||
    message.includes("networkerror") ||
    message.includes("status 500") ||
    message.includes("status 502") ||
    message.includes("status 503") ||
    message.includes("status 504")
  );
}

async function loadInitialStateWithRetry(): Promise<{
  health: HealthResponse;
  config: AppConfig;
  jobs: JobSummary[];
}> {
  let lastError: unknown = null;

  for (let attempt = 1; attempt <= STARTUP_RETRY_ATTEMPTS; attempt += 1) {
    try {
      const [health, config, jobs] = await Promise.all([
        getHealth(),
        getConfig(),
        listJobs(),
      ]);
      return { health, config, jobs };
    } catch (error) {
      lastError = error;

      if (
        attempt === STARTUP_RETRY_ATTEMPTS ||
        !isTransientBootstrapError(error)
      ) {
        throw error;
      }

      await sleep(STARTUP_RETRY_DELAY_MS);
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`Failed to load ${APP_NAME}.`);
}

async function syncAuthenticationStatus(
  server: ServerConfig
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server);
  state.lastAuthValidationServer = server;
  state.lastAuthValidationResult = result;
  state.connectionSummary = result;
  state.lastConnectionState = result.state;

  if (result.ok) {
    await loadDatastreams();
  } else {
    state.datastreams = [];
    state.datastreamsError = null;
  }

  return result;
}

async function loadDatastreams(): Promise<void> {
  try {
    state.datastreams = await getDatastreams();
    state.datastreamsError = null;
  } catch (error) {
    state.datastreams = [];
    state.datastreamsError =
      error instanceof Error
        ? error.message
        : "Couldn't load HydroServer datastreams.";
  }
}

async function bootstrap(): Promise<void> {
  state.loading = true;
  state.bootstrapError = null;
  state.welcomeFeedback = null;
  state.settingsFeedback = null;
  render();

  try {
    const { health, config, jobs } = await loadInitialStateWithRetry();
    state.health = health;
    state.config = config;
    state.jobs = jobs;
    state.lastConnectionState = health.connection.state;

    if (serverConfigured(config.server)) {
      await syncAuthenticationStatus(config.server);
    }
  } catch (error) {
    state.bootstrapError =
      error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`;
  } finally {
    state.loading = false;
    render();
  }
}

async function refreshJobs(): Promise<void> {
  if (state.bootstrapError || state.loading) {
    return;
  }

  try {
    state.jobs = await listJobs();
    render();
  } catch {
    // Keep existing UI state on polling failure.
  }
}

function updatePipelineField(name: string, value: string): void {
  switch (name) {
    case "pipeline_name":
      state.pipelineForm.name = value;
      break;
    case "file_path":
      state.pipelineForm.filePath = value;
      break;
    case "schedule_minutes":
      state.pipelineForm.scheduleMinutes = Number(value) || 15;
      break;
    case "header_row":
      state.pipelineForm.headerRow = Number(value) || 1;
      syncPipelineSelectionsWithPreview();
      break;
    case "data_start_row":
      state.pipelineForm.dataStartRow = Number(value) || 1;
      break;
    case "delimiter":
      state.pipelineForm.delimiter = value || ",";
      syncPipelineSelectionsWithPreview();
      break;
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value;
      initializeMappings(previewHeaders());
      render();
      break;
    case "timestamp_format":
      state.pipelineForm.timestampFormat = value;
      break;
    case "timezone":
      state.pipelineForm.timezone = value;
      break;
    default:
      break;
  }
}

function validatePipeline(): string[] {
  const errors: string[] = [];
  const headers = previewHeaders();
  const selectedMappings = state.pipelineForm.mappings.filter(
    (mapping) => mapping.datastreamId
  );
  const datastreamIds = new Set(
    state.datastreams.map((datastream) => datastream.id)
  );
  const seenTargets = new Set<string>();

  if (!connected()) {
    errors.push("Connect to HydroServer before saving a pipeline.");
  }

  if (!state.pipelineForm.name.trim()) {
    errors.push("Give the pipeline a name.");
  }

  if (!state.pipelineForm.filePath.trim()) {
    errors.push(`Choose the CSV file ${APP_NAME} should watch.`);
  }

  if (!state.pipelinePreview) {
    errors.push("Load a CSV preview before saving the pipeline.");
  }

  if (state.pipelineForm.headerRow < 1) {
    errors.push("Header row must be 1 or greater.");
  }

  if (state.pipelineForm.dataStartRow <= state.pipelineForm.headerRow) {
    errors.push("Data start row must come after the header row.");
  }

  if (
    headers.length > 0 &&
    !headers.includes(state.pipelineForm.timestampColumn)
  ) {
    errors.push(
      "Choose a timestamp column that exists in the previewed CSV header."
    );
  }

  if (selectedMappings.length === 0) {
    errors.push("Map at least one source column to a HydroServer datastream.");
  }

  for (const mapping of selectedMappings) {
    if (!datastreamIds.has(mapping.datastreamId)) {
      errors.push(
        `The selected target for ${mapping.csvColumn} is not a valid HydroServer datastream.`
      );
    }

    if (seenTargets.has(mapping.datastreamId)) {
      errors.push(
        "Each target datastream can only be mapped once in this first-run flow."
      );
    }

    seenTargets.add(mapping.datastreamId);
  }

  return errors;
}

async function loadPipelinePreview(path: string): Promise<void> {
  if (!path.trim()) {
    state.pipelineFeedback = {
      tone: "error",
      message: "Enter or choose a CSV file path first.",
    };
    render();
    return;
  }

  try {
    const preview = await getCsvPreview(path.trim(), 50);
    applyPreview(path.trim(), preview);
    state.pipelineErrors = [];
    state.pipelineFeedback = {
      tone: "success",
      message:
        "Preview loaded. Click the structure on the right to finish the form.",
    };
  } catch (error) {
    state.pipelinePreview = null;
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't preview that CSV file.",
    };
  }

  render();
}

async function browseForCsvPath(): Promise<void> {
  try {
    const dialog = await import("@tauri-apps/plugin-dialog");
    const selection = await dialog.open({
      directory: false,
      multiple: false,
      filters: [{ name: "CSV files", extensions: ["csv", "txt"] }],
    });

    if (typeof selection !== "string" || !selection) {
      return;
    }

    state.pipelineForm.filePath = selection;
    if (!state.pipelineForm.name.trim()) {
      state.pipelineForm.name = basename(selection).replace(/\.[^.]+$/, "");
    }

    await loadPipelinePreview(selection);
  } catch {
    state.pipelineFeedback = {
      tone: "info",
      message:
        "The native file picker is only available in the desktop app. Enter the CSV path manually if you're using the browser preview.",
    };
    render();
  }
}

async function saveAuthenticatedServerConfig(
  form: HTMLFormElement
): Promise<void> {
  if (state.authSubmitting) {
    return;
  }

  const payload = readServerConfigForm(form);
  setServerDraft(payload);

  const feedbackKey = fieldFormFeedbackTarget(form.id);

  state[feedbackKey] = null;
  resetStateAuthFieldStates(payload.auth_type);

  if (!validateAuthFieldsForSubmit(payload, markField)) {
    render();
    return;
  }

  try {
    await runAuthSubmission({
      render,
      setSubmitting: (value) => {
        state.authSubmitting = value;
      },
      action: async () => {
        const urlValidation = await validateServerUrl(payload.url);
        if (!urlValidation.ok) {
          clearAuthValidationCache();
          markField("url", "invalid", urlValidation.message);
          state[feedbackKey] = {
            tone: "error",
            message: urlValidation.message,
          };
          return;
        }

        markField("url", "valid");

        const result = await syncAuthenticationStatus(payload);
        applyConnectionValidationResult(payload, result, markField);
        if (!result.ok) {
          state[feedbackKey] = { tone: "error", message: result.message };
          return;
        }

        state.config = await updateServerConfig(payload);
        state.authDraft = {
          ...emptyServerConfig(),
          ...state.config.server,
        };
        state[feedbackKey] = { tone: "success", message: result.message };
        state.settingsEditMode = false;

        if (state.jobs.length === 0) {
          navigate("jobs-new");
        } else {
          navigate("dashboard");
        }
      },
    });
  } catch (error) {
    clearAuthValidationCache();
    state[feedbackKey] = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't verify the HydroServer connection.",
    };
    state.lastConnectionState = "error";
    render();
  }
}

async function disconnectHydroServer(): Promise<void> {
  try {
    state.config = await clearServerConfig();
    state.authDraft = emptyServerConfig();
    state.connectionSummary = null;
    state.lastConnectionState = "not_configured";
    state.datastreams = [];
    state.datastreamsError = null;
    state.welcomeFeedback = null;
    state.settingsFeedback = null;
    state.settingsEditMode = false;
    resetStateAuthFieldStates("apikey");
    clearAuthValidationCache();
    navigate("welcome");
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't disconnect from HydroServer right now.",
    };
  }

  render();
}

async function savePipeline(): Promise<void> {
  state.pipelineErrors = validatePipeline();

  if (state.pipelineErrors.length > 0) {
    state.pipelineFeedback = {
      tone: "error",
      message: `${APP_NAME} needs a little more information before it can save this pipeline.`,
    };
    render();
    return;
  }

  const mappedColumns = state.pipelineForm.mappings
    .filter((mapping) => mapping.datastreamId)
    .map((mapping) => {
      const datastream = state.datastreams.find(
        (item) => item.id === mapping.datastreamId
      );
      return {
        csv_column: mapping.csvColumn,
        datastream_id: mapping.datastreamId,
        datastream_name: datastream?.name ?? mapping.datastreamId,
      };
    });

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
    });

    state.jobs = [...state.jobs, created];
    state.pipelineForm = createEmptyPipelineForm();
    state.pipelinePreview = null;
    state.pipelineSelectionTarget = null;
    state.pipelineErrors = [];
    state.pipelineFeedback = { tone: "success", message: "Pipeline saved." };
    navigate("dashboard");
  } catch (error) {
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't save that pipeline.",
    };
  }

  render();
}

window.addEventListener("hashchange", () => {
  state.settingsFeedback = null;
  render();
});

mainContent.addEventListener("submit", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLFormElement)) {
    return;
  }

  event.preventDefault();

  if (target.id === "welcome-form") {
    void saveAuthenticatedServerConfig(target);
    return;
  }

  if (target.id === "settings-form") {
    void saveAuthenticatedServerConfig(target);
    return;
  }

  if (target.id === "pipeline-form") {
    void savePipeline();
  }
});

mainContent.addEventListener("input", (event) => {
  const target = event.target;

  if (
    !(
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    )
  ) {
    return;
  }

  if (
    target.form?.id === "welcome-form" ||
    target.form?.id === "settings-form"
  ) {
    const form = target.form;
    setServerDraft(readServerConfigForm(form));
    clearAuthFormFeedback(form.id);
    clearAuthValidationCache();

    if (
      target instanceof HTMLInputElement &&
      (target.name === "url" ||
        target.name === "api_key" ||
        target.name === "username" ||
        target.name === "password")
    ) {
      markField(target.name, "idle");
    }
    return;
  }

  if (target.form?.id !== "pipeline-form") {
    return;
  }

  state.pipelineFeedback = null;
  state.pipelineErrors = [];

  const mappingColumn = target.dataset.mappingColumn;
  if (mappingColumn) {
    const mapping = state.pipelineForm.mappings.find(
      (item) => item.csvColumn === mappingColumn
    );
    if (mapping) {
      mapping.datastreamId = target.value;
    }
    render();
    return;
  }

  updatePipelineField(target.name, target.value);

  if (
    target.name === "header_row" ||
    target.name === "data_start_row" ||
    target.name === "delimiter" ||
    target.name === "timestamp_column"
  ) {
    render();
  }
});

mainContent.addEventListener("click", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return;
  }

  const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
  const jobId = target.closest<HTMLElement>("[data-job-id]")?.dataset.jobId;

  if (!action) {
    return;
  }

  if (action === "retry-bootstrap") {
    void bootstrap();
    return;
  }

  if (action === "toggle-auth-mode") {
    const form = target.closest<HTMLFormElement>("form");
    if (!form) {
      return;
    }

    const nextServer = readServerConfigForm(form);
    const nextAuthType: AuthType =
      nextServer.auth_type === "apikey" ? "userpass" : "apikey";
    setServerDraft({
      ...nextServer,
      auth_type: nextAuthType,
    });
    resetStateAuthFieldStates(nextAuthType);

    clearAuthFormFeedback(form.id);
    clearAuthValidationCache();

    render();
    return;
  }

  if (action === "disconnect") {
    void disconnectHydroServer();
    return;
  }

  if (action === "change-credentials") {
    state.authDraft = {
      ...emptyServerConfig(),
      ...(state.config?.server ?? {}),
    };
    state.settingsEditMode = true;
    navigate("settings");
    render();
    return;
  }

  if (action === "cancel-credential-edit") {
    state.authDraft = {
      ...emptyServerConfig(),
      ...(state.config?.server ?? {}),
    };
    state.settingsEditMode = false;
    render();
    return;
  }

  if (action === "browse-csv") {
    void browseForCsvPath();
    return;
  }

  if (action === "load-preview") {
    void loadPipelinePreview(state.pipelineForm.filePath);
    return;
  }

  if (action === "toggle-preview-picker") {
    if (!state.pipelinePreview) {
      state.pipelineFeedback = {
        tone: "info",
        message: "Load a CSV preview first.",
      };
      render();
      return;
    }

    const pickerTarget = target.closest<HTMLElement>("[data-picker-target]")
      ?.dataset.pickerTarget;
    if (
      pickerTarget !== "header-row" &&
      pickerTarget !== "data-start-row" &&
      pickerTarget !== "timestamp-column"
    ) {
      return;
    }

    state.pipelineSelectionTarget =
      state.pipelineSelectionTarget === pickerTarget ? null : pickerTarget;
    render();
    return;
  }

  if (action === "pick-preview-line") {
    const lineNumber = Number(
      target.closest<HTMLElement>("[data-preview-line]")?.dataset.previewLine
    );

    if (Number.isFinite(lineNumber)) {
      applyPreviewLineSelection(lineNumber);
    }
    return;
  }

  if (action === "pick-preview-column") {
    const columnName =
      target.closest<HTMLElement>("[data-preview-column]")?.dataset
        .previewColumn ?? "";

    if (columnName) {
      applyPreviewColumnSelection(columnName);
    }
    return;
  }

  if (!jobId) {
    return;
  }

  if (action === "run-job") {
    void handleRunJob(jobId);
    return;
  }

  if (action === "toggle-job") {
    void handleToggleJob(jobId);
    return;
  }

  if (action === "delete-job") {
    void handleDeleteJob(jobId);
  }
});

async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId);
    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

async function handleToggleJob(jobId: string): Promise<void> {
  const job = state.jobs.find((item) => item.id === jobId);
  if (!job) {
    return;
  }

  try {
    if (job.enabled) {
      await disableJob(jobId);
    } else {
      await enableJob(jobId);
    }

    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

async function handleDeleteJob(jobId: string): Promise<void> {
  const confirmed = window.confirm("Delete this pipeline?");
  if (!confirmed) {
    return;
  }

  try {
    await deleteJob(jobId);
    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

void bootstrap();
