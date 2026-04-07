import { computed } from "vue";

import { getCsvPreview, createJob } from "../api";
import { navigate } from "../router";
import {
  state,
  createEmptyPipelineForm,
  PREVIEW_PAGE_SIZE,
} from "./state";

// ── Utilities (pipeline-local) ─────────────────────────────────────────────
function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function parseDelimitedLine(line: string, delimiter: string): string[] {
  if (!delimiter) return [line];
  const cells: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '"') {
      if (inQuotes && line[i + 1] === '"') {
        current += '"';
        i++;
      } else inQuotes = !inQuotes;
      continue;
    }
    if (!inQuotes && line.startsWith(delimiter, i)) {
      cells.push(current);
      current = "";
      i += delimiter.length - 1;
      continue;
    }
    current += char;
  }
  cells.push(current);
  return cells;
}

function normalizeHeaderName(value: string, index: number): string {
  return value.trim() || `Column ${index + 1}`;
}

// ── Computed ───────────────────────────────────────────────────────────────
export const parsedPreviewRows = computed(() => {
  if (!state.pipelinePreview) return [];
  return state.pipelinePreview.raw_lines.map((line) =>
    parseDelimitedLine(line, state.pipelineForm.delimiter)
  );
});

export const previewHeaders = computed(() => {
  const rows = parsedPreviewRows.value;
  const columnCount = rows.reduce((max, row) => Math.max(max, row.length), 0);

  if (!state.pipelineForm.hasHeaderRow) {
    const dataRows = rows.slice(
      Math.max(state.pipelineForm.dataStartRow - 1, 0)
    );
    const count = (dataRows.length > 0 ? dataRows : rows).reduce(
      (max, row) => Math.max(max, row.length),
      0
    );
    return Array.from({ length: count }, (_, i) => `Column ${i + 1}`);
  }

  const headerRow = rows[state.pipelineForm.headerRow - 1] ?? [];
  return Array.from({ length: columnCount }, (_, i) =>
    normalizeHeaderName(headerRow[i] ?? "", i)
  );
});

// ── Internal helpers ───────────────────────────────────────────────────────
function pipelineMappingsByColumn(): Map<string, string> {
  return new Map(
    state.pipelineForm.mappings.map((m) => [m.csvColumn, m.datastreamId])
  );
}

function initializeMappings(headers: string[]): void {
  const existing = pipelineMappingsByColumn();
  state.pipelineForm.mappings = headers
    .filter((h) => h !== state.pipelineForm.timestampColumn)
    .map((h) => ({ csvColumn: h, datastreamId: existing.get(h) ?? "" }));
}

function syncSelectionsWithPreview(): void {
  const headers = previewHeaders.value;
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

export function canShowMorePreviewLines(): boolean {
  if (!state.pipelinePreview) return false;
  return (
    state.pipelinePreview.raw_lines.length < state.pipelinePreview.total_lines
  );
}

// ── Row/column selection ───────────────────────────────────────────────────
export function updateHeaderRowFromPreview(lineNumber: number): void {
  state.pipelineForm.hasHeaderRow = true;
  state.pipelineForm.headerRow = lineNumber;
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1;
  }
  syncSelectionsWithPreview();
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
  syncSelectionsWithPreview();
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
  )
    return;
  state.pipelineForm.timestampColumn = columnName;
  initializeMappings(previewHeaders.value);
  state.pipelineSelectionTarget = null;
}

export function setPipelineHasHeaderRow(enabled: boolean): void {
  state.pipelineForm.hasHeaderRow = enabled;
  if (!enabled && state.pipelineSelectionTarget === "header-row") {
    state.pipelineSelectionTarget = null;
  }
  if (
    enabled &&
    state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow
  ) {
    state.pipelineForm.headerRow = Math.max(
      1,
      state.pipelineForm.dataStartRow - 1
    );
  }
  syncSelectionsWithPreview();
}

// ── Form field updates ─────────────────────────────────────────────────────
export function updatePipelineField(name: string, value: string): void {
  state.pipelineFeedback = null;
  state.pipelineErrors = [];
  switch (name) {
    case "pipeline_name":
      state.pipelineForm.name = value;
      break;
    case "file_path":
      state.pipelineForm.filePath = value;
      state.pipelinePreview = null;
      state.pipelineSelectionTarget = null;
      state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
      break;
    case "schedule_minutes":
      state.pipelineForm.scheduleMinutes = Number(value) || 15;
      break;
    case "header_row":
      state.pipelineForm.headerRow = Number(value) || 1;
      syncSelectionsWithPreview();
      break;
    case "data_start_row":
      state.pipelineForm.dataStartRow = Number(value) || 1;
      syncSelectionsWithPreview();
      break;
    case "delimiter":
      state.pipelineForm.delimiter = value || ",";
      syncSelectionsWithPreview();
      break;
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value;
      initializeMappings(previewHeaders.value);
      break;
    case "timestamp_format":
      state.pipelineForm.timestampFormat = value;
      break;
    case "timezone":
      state.pipelineForm.timezone = value;
      break;
  }
}

export function updateMapping(csvColumn: string, datastreamId: string): void {
  state.pipelineFeedback = null;
  state.pipelineErrors = [];
  const mapping = state.pipelineForm.mappings.find(
    (m) => m.csvColumn === csvColumn
  );
  if (mapping) mapping.datastreamId = datastreamId;
}

// ── Async actions ──────────────────────────────────────────────────────────
export async function loadPipelinePreview(
  path: string,
  rows = PREVIEW_PAGE_SIZE
): Promise<void> {
  if (!path.trim()) {
    state.pipelineFeedback = {
      tone: "error",
      message: "Enter or choose a CSV file path first.",
    };
    return;
  }
  try {
    const preview = await getCsvPreview(path.trim(), rows);
    state.pipelinePreview = preview;
    state.pipelineForm.filePath = path.trim();
    state.pipelineForm.hasHeaderRow = preview.detected_header_row !== null;
    state.pipelineForm.headerRow =
      preview.detected_header_row ?? state.pipelineForm.headerRow;
    state.pipelineForm.dataStartRow =
      preview.detected_data_start_row ?? state.pipelineForm.dataStartRow;
    state.pipelineForm.delimiter =
      preview.detected_delimiter || state.pipelineForm.delimiter;
    state.pipelineSelectionTarget = null;
    if (!state.pipelineForm.name.trim()) {
      state.pipelineForm.name = basename(path.trim()).replace(/\.[^.]+$/, "");
    }
    syncSelectionsWithPreview();
    state.pipelinePreviewRowsRequested = rows;
    state.pipelineErrors = [];
    state.pipelineFeedback = null;
  } catch (error) {
    state.pipelinePreview = null;
    state.pipelineSelectionTarget = null;
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't preview that CSV file.",
    };
  }
}

export async function showMorePreviewLines(): Promise<void> {
  if (!state.pipelinePreview) return;
  const nextRows = Math.min(
    state.pipelinePreviewRowsRequested + PREVIEW_PAGE_SIZE,
    state.pipelinePreview.total_lines
  );
  await loadPipelinePreview(state.pipelineForm.filePath, nextRows);
}

export async function browseForCsvPath(): Promise<void> {
  try {
    const dialog = await import("@tauri-apps/plugin-dialog");
    const selection = await dialog.open({
      directory: false,
      multiple: false,
      filters: [{ name: "CSV files", extensions: ["csv", "txt"] }],
    });
    if (typeof selection !== "string" || !selection) return;
    updatePipelineField("file_path", selection);
    if (!state.pipelineForm.name.trim()) {
      state.pipelineForm.name = basename(selection).replace(/\.[^.]+$/, "");
    }
    await loadPipelinePreview(selection);
  } catch {
    state.pipelineFeedback = {
      tone: "info",
      message:
        "The native file picker is only available in the desktop app. Enter the CSV path manually.",
    };
  }
}

function validatePipeline(): string[] {
  const errors: string[] = [];
  const headers = previewHeaders.value;
  const selectedMappings = state.pipelineForm.mappings.filter(
    (m) => m.datastreamId
  );
  const datastreamIds = new Set(state.datastreams.map((d) => d.id));
  const seenTargets = new Set<string>();
  const isConnected =
    state.connectionSummary?.ok === true &&
    state.lastConnectionState === "connected";

  if (!isConnected)
    errors.push("Connect to HydroServer before saving a pipeline.");
  if (!state.pipelineForm.name.trim()) errors.push("Give the pipeline a name.");
  if (!state.pipelineForm.filePath.trim())
    errors.push("Choose the CSV file to watch.");
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
  for (const m of selectedMappings) {
    if (!datastreamIds.has(m.datastreamId)) {
      errors.push(
        `The selected target for "${m.csvColumn}" is not a valid datastream.`
      );
    }
    if (seenTargets.has(m.datastreamId)) {
      errors.push("Each datastream can only be mapped to one source column.");
    }
    seenTargets.add(m.datastreamId);
  }
  return errors;
}

export async function submitPipeline(): Promise<void> {
  state.pipelineErrors = validatePipeline();
  if (state.pipelineErrors.length > 0) {
    state.pipelineFeedback = {
      tone: "error",
      message: "Fix the errors below before saving.",
    };
    return;
  }

  const mappedColumns = state.pipelineForm.mappings
    .filter((m) => m.datastreamId)
    .map((m) => ({
      csv_column: m.csvColumn,
      datastream_id: m.datastreamId,
      datastream_name:
        state.datastreams.find((d) => d.id === m.datastreamId)?.name ??
        m.datastreamId,
    }));

  try {
    const created = await createJob({
      name: state.pipelineForm.name.trim(),
      enabled: true,
      file_path: state.pipelineForm.filePath.trim(),
      schedule_minutes: state.pipelineForm.scheduleMinutes,
      file_config: {
        header_row: state.pipelineForm.hasHeaderRow
          ? state.pipelineForm.headerRow
          : 0,
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
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
    state.pipelineErrors = [];
    state.wizardStep = "file-config";
    navigate("dashboard");
  } catch (error) {
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't save that pipeline.",
    };
  }
}

export function advanceToMapping(): void {
  state.wizardStep = "column-mapping";
  state.pipelineErrors = [];
  state.pipelineFeedback = null;
}

export function backToFileConfig(): void {
  state.wizardStep = "file-config";
  state.pipelineErrors = [];
  state.pipelineFeedback = null;
}
