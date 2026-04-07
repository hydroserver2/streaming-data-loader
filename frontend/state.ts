import type {
  ConnectionState,
  AuthType,
  ServerConfig,
  AppConfig,
  HealthResponse,
  DatastreamSummary,
  CsvPreviewResponse,
  JobSummary,
  ConnectionTestResponse,
} from "./api";
import {
  createAuthFieldStates,
  resetAuthFieldStates,
  type AuthFieldName,
  type AuthFieldStates,
  type FieldValidationState,
} from "./auth-submit";
import { getRouteFromHash, type AppRoute } from "./router";
import { parseDelimitedLine, basename, type Feedback } from "./components/helpers";

// ── Wizard step ────────────────────────────────────────────────────────────
export type OnboardingStep = "file-config" | "column-mapping";

// ── Pipeline form ──────────────────────────────────────────────────────────
export type PipelineMappingDraft = {
  csvColumn: string;
  datastreamId: string;
};

export type PipelineFormState = {
  name: string;
  filePath: string;
  scheduleMinutes: number;
  hasHeaderRow: boolean;
  headerRow: number;
  dataStartRow: number;
  delimiter: string;
  timestampColumn: string;
  timestampFormat: string;
  timezone: string;
  mappings: PipelineMappingDraft[];
};

// ── Preview drag types ─────────────────────────────────────────────────────
export type PreviewSelectionTarget =
  | "header-row"
  | "data-start-row"
  | "timestamp-column"
  | null;

export type PreviewRowSelectionTarget = Exclude<
  PreviewSelectionTarget,
  "timestamp-column" | null
>;

export type PreviewDragState = {
  target: PreviewRowSelectionTarget;
  lineNumber: number;
  pointerId: number;
  moved: boolean;
};

export type PreviewColumnDragState = {
  columnName: string;
  pointerId: number;
  moved: boolean;
};

// ── Global UI state ────────────────────────────────────────────────────────
export type UiState = {
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
  onboardingStep: OnboardingStep;
  pipelineForm: PipelineFormState;
  pipelinePreview: CsvPreviewResponse | null;
  pipelineErrors: string[];
  datastreamsError: string | null;
  authDraft: ServerConfig;
  authFieldStates: AuthFieldStates;
  authSubmitting: boolean;
  lastAuthValidationServer: ServerConfig | null;
  lastAuthValidationResult: ConnectionTestResponse | null;
  pipelineSelectionTarget: PreviewSelectionTarget;
  pipelineDrag: PreviewDragState | null;
  pipelineColumnDrag: PreviewColumnDragState | null;
  pipelinePreviewRowsRequested: number;
};

// ── Constants ──────────────────────────────────────────────────────────────
export const PREVIEW_PAGE_SIZE = 50;

// ── Factories ─────────────────────────────────────────────────────────────
export function emptyServerConfig(): ServerConfig {
  return {
    auth_type: "apikey",
    url: "",
    api_key: "",
    username: "",
    password: "",
    workspace_id: "",
  };
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
  };
}

// ── Singleton state ────────────────────────────────────────────────────────
export const state: UiState = {
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
  onboardingStep: "file-config",
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
  pipelineDrag: null,
  pipelineColumnDrag: null,
  pipelinePreviewRowsRequested: PREVIEW_PAGE_SIZE,
};

// ── Computed selectors ─────────────────────────────────────────────────────
export function connected(): boolean {
  return (
    state.connectionSummary?.ok === true &&
    state.lastConnectionState === "connected"
  );
}

export function serverConfigured(server: ServerConfig | null | undefined): boolean {
  if (!server?.url.trim()) return false;
  if (server.auth_type === "userpass") {
    return Boolean(server.username.trim() && server.password.trim());
  }
  return Boolean(server.api_key.trim());
}

export function onboardingRoute(route: AppRoute): boolean {
  return route === "welcome" || (route === "jobs-new" && state.jobs.length === 0);
}

function normalizePreviewHeaderName(value: string, index: number): string {
  return value.trim() || `Column ${index + 1}`;
}

export function parsedPreviewRows(): string[][] {
  if (!state.pipelinePreview) return [];
  return state.pipelinePreview.raw_lines.map((line) =>
    parseDelimitedLine(line, state.pipelineForm.delimiter)
  );
}

export function previewHeaders(): string[] {
  const rows = parsedPreviewRows();
  const columnCount = rows.reduce((max, row) => Math.max(max, row.length), 0);

  if (!state.pipelineForm.hasHeaderRow) {
    const dataRows = rows.slice(Math.max(state.pipelineForm.dataStartRow - 1, 0));
    const dataColumnCount = (dataRows.length > 0 ? dataRows : rows).reduce(
      (max, row) => Math.max(max, row.length),
      0
    );
    return Array.from({ length: dataColumnCount }, (_, i) => `Column ${i + 1}`);
  }

  const headerRow = rows[state.pipelineForm.headerRow - 1] ?? [];
  return Array.from({ length: columnCount }, (_, i) =>
    normalizePreviewHeaderName(headerRow[i] ?? "", i)
  );
}

export function pipelineMappingsByColumn(): Map<string, string> {
  return new Map(state.pipelineForm.mappings.map((m) => [m.csvColumn, m.datastreamId]));
}

export function activeTimestampColumn(): string {
  return state.pipelineColumnDrag?.columnName ?? state.pipelineForm.timestampColumn;
}

export function previewHandleLine(target: PreviewRowSelectionTarget): number | null {
  if (state.pipelineDrag?.target === target) return state.pipelineDrag.lineNumber;
  if (target === "header-row") {
    return state.pipelineForm.hasHeaderRow ? state.pipelineForm.headerRow : null;
  }
  return state.pipelineForm.dataStartRow;
}

export function activePreviewRowTarget(): PreviewRowSelectionTarget | null {
  if (state.pipelineDrag) return state.pipelineDrag.target;
  return state.pipelineSelectionTarget === "header-row" ||
    state.pipelineSelectionTarget === "data-start-row"
    ? state.pipelineSelectionTarget
    : null;
}

export function previewCommittedHandleLine(target: PreviewRowSelectionTarget): number | null {
  if (target === "header-row") {
    return state.pipelineForm.hasHeaderRow ? state.pipelineForm.headerRow : null;
  }
  return state.pipelineForm.dataStartRow;
}

export function canShowMorePreviewLines(): boolean {
  if (!state.pipelinePreview) return false;
  return state.pipelinePreview.raw_lines.length < state.pipelinePreview.total_lines;
}

// ── Auth mutations ─────────────────────────────────────────────────────────
export function setServerDraft(server: ServerConfig): void {
  state.authDraft = { ...server };
}

export function markField(
  field: AuthFieldName,
  nextState: FieldValidationState["state"],
  message: string | null = null
): void {
  state.authFieldStates[field] = { state: nextState, message };
}

export function resetStateAuthFieldStates(authType: AuthType): void {
  resetAuthFieldStates(state.authFieldStates, authType);
}

export function clearAuthValidationCache(): void {
  state.lastAuthValidationServer = null;
  state.lastAuthValidationResult = null;
}

export function clearAuthFormFeedback(formId: string): void {
  if (formId === "welcome-form") {
    state.welcomeFeedback = null;
  } else {
    state.settingsFeedback = null;
  }
}

export function readServerConfigForm(form: HTMLFormElement): ServerConfig {
  const data = new FormData(form);
  const authType = data.get("auth_type") === "userpass" ? "userpass" : "apikey";
  return {
    auth_type: authType,
    url: String(data.get("url") ?? "").trim(),
    api_key:
      authType === "apikey"
        ? String(data.get("api_key") ?? "").trim()
        : state.authDraft.api_key,
    username:
      authType === "userpass"
        ? String(data.get("username") ?? "").trim()
        : state.authDraft.username,
    password:
      authType === "userpass"
        ? String(data.get("password") ?? "").trim()
        : state.authDraft.password,
    workspace_id: "",
  };
}

// ── Pipeline mutations ─────────────────────────────────────────────────────
export function initializeMappings(headers: string[]): void {
  const existing = pipelineMappingsByColumn();
  state.pipelineForm.mappings = headers
    .filter((h) => h !== state.pipelineForm.timestampColumn)
    .map((h) => ({ csvColumn: h, datastreamId: existing.get(h) ?? "" }));
}

export function syncPipelineSelectionsWithPreview(): void {
  const headers = previewHeaders();
  if (headers.length === 0) {
    state.pipelineForm.mappings = [];
    return;
  }
  const preferred =
    headers.find((h) => h.toLowerCase().includes("time")) ?? headers[0];
  state.pipelineForm.timestampColumn = headers.includes(
    state.pipelineForm.timestampColumn
  )
    ? state.pipelineForm.timestampColumn
    : preferred;
  initializeMappings(headers);
}

export function applyPreview(path: string, preview: CsvPreviewResponse): void {
  state.pipelinePreview = preview;
  state.pipelineForm.filePath = path;
  state.pipelineForm.hasHeaderRow = preview.detected_header_row !== null;
  state.pipelineForm.headerRow =
    preview.detected_header_row ?? state.pipelineForm.headerRow;
  state.pipelineForm.dataStartRow =
    preview.detected_data_start_row ?? state.pipelineForm.dataStartRow;
  state.pipelineForm.delimiter =
    preview.detected_delimiter || state.pipelineForm.delimiter;
  state.pipelineSelectionTarget = null;
  state.pipelineDrag = null;
  state.pipelineColumnDrag = null;

  if (!state.pipelineForm.name.trim()) {
    state.pipelineForm.name = basename(path).replace(/\.[^.]+$/, "");
  }

  syncPipelineSelectionsWithPreview();
}

export function updateHeaderRowFromPreview(lineNumber: number): void {
  state.pipelineForm.hasHeaderRow = true;
  state.pipelineForm.headerRow = lineNumber;
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1;
  }
  syncPipelineSelectionsWithPreview();
}

export function updateDataStartRowFromPreview(lineNumber: number): void {
  state.pipelineForm.dataStartRow = Math.max(
    state.pipelineForm.hasHeaderRow ? 2 : 1,
    lineNumber
  );
  if (
    state.pipelineForm.hasHeaderRow &&
    state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow
  ) {
    state.pipelineForm.headerRow = state.pipelineForm.dataStartRow - 1;
  }
  syncPipelineSelectionsWithPreview();
}

export function setPipelineHasHeaderRow(enabled: boolean): void {
  state.pipelineForm.hasHeaderRow = enabled;
  if (!enabled && state.pipelineSelectionTarget === "header-row") {
    state.pipelineSelectionTarget = null;
  }
  if (!enabled && state.pipelineDrag?.target === "header-row") {
    state.pipelineDrag = null;
  }
  if (enabled && state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow) {
    state.pipelineForm.headerRow = Math.max(1, state.pipelineForm.dataStartRow - 1);
  }
  syncPipelineSelectionsWithPreview();
}

export function applyPreviewLineSelection(lineNumber: number): void {
  if (state.pipelineSelectionTarget === "header-row") {
    updateHeaderRowFromPreview(lineNumber);
    state.pipelineSelectionTarget = null;
  } else if (state.pipelineSelectionTarget === "data-start-row") {
    updateDataStartRowFromPreview(lineNumber);
    state.pipelineSelectionTarget = null;
  }
}

export function applyPreviewColumnSelection(columnName: string): void {
  if (
    state.pipelineSelectionTarget &&
    state.pipelineSelectionTarget !== "timestamp-column"
  ) {
    return;
  }
  state.pipelineForm.timestampColumn = columnName;
  initializeMappings(previewHeaders());
  state.pipelineSelectionTarget = null;
  state.pipelineColumnDrag = null;
}

export function updatePipelineField(name: string, value: string): void {
  switch (name) {
    case "pipeline_name":
      state.pipelineForm.name = value;
      break;
    case "file_path":
      state.pipelineForm.filePath = value;
      state.pipelinePreview = null;
      state.pipelineErrors = [];
      state.pipelineSelectionTarget = null;
      state.pipelineDrag = null;
      state.pipelineColumnDrag = null;
      state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
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
      syncPipelineSelectionsWithPreview();
      break;
    case "delimiter":
      state.pipelineForm.delimiter = value || ",";
      syncPipelineSelectionsWithPreview();
      break;
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value;
      initializeMappings(previewHeaders());
      state.pipelineColumnDrag = null;
      break;
    case "timestamp_format":
      state.pipelineForm.timestampFormat = value;
      break;
    case "timezone":
      state.pipelineForm.timezone = value;
      break;
  }
}

export function resetPipelineState(): void {
  state.pipelineForm = createEmptyPipelineForm();
  state.pipelinePreview = null;
  state.pipelineSelectionTarget = null;
  state.pipelineDrag = null;
  state.pipelineColumnDrag = null;
  state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
  state.pipelineErrors = [];
  state.pipelineFeedback = null;
  state.onboardingStep = "file-config";
}

export function validatePipeline(): string[] {
  const errors: string[] = [];
  const headers = previewHeaders();
  const selectedMappings = state.pipelineForm.mappings.filter((m) => m.datastreamId);
  const datastreamIds = new Set(state.datastreams.map((d) => d.id));
  const seenTargets = new Set<string>();

  if (!connected()) errors.push("Connect to HydroServer before saving a pipeline.");
  if (!state.pipelineForm.name.trim()) errors.push("Give the pipeline a name.");
  if (!state.pipelineForm.filePath.trim()) errors.push("Choose the CSV file to watch.");
  if (!state.pipelinePreview) errors.push("Load a CSV preview before saving.");
  if (state.pipelineForm.hasHeaderRow && state.pipelineForm.headerRow < 1) {
    errors.push("Header row must be 1 or greater.");
  }
  if (
    state.pipelineForm.hasHeaderRow &&
    state.pipelineForm.dataStartRow <= state.pipelineForm.headerRow
  ) {
    errors.push("Data start row must come after the header row.");
  }
  if (!state.pipelineForm.hasHeaderRow && state.pipelineForm.dataStartRow < 1) {
    errors.push("Data start row must be 1 or greater.");
  }
  if (headers.length > 0 && !headers.includes(state.pipelineForm.timestampColumn)) {
    errors.push("Choose a timestamp column that exists in the CSV header.");
  }
  if (selectedMappings.length === 0) {
    errors.push("Map at least one source column to a HydroServer datastream.");
  }
  for (const mapping of selectedMappings) {
    if (!datastreamIds.has(mapping.datastreamId)) {
      errors.push(
        `The selected target for "${mapping.csvColumn}" is not a valid datastream.`
      );
    }
    if (seenTargets.has(mapping.datastreamId)) {
      errors.push("Each datastream can only be mapped to one source column.");
    }
    seenTargets.add(mapping.datastreamId);
  }

  return errors;
}
