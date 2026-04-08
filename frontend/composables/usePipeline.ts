import { computed } from "vue"

import type { CsvTransformerSettings } from "../api"
import { getCsvPreview } from "../api"
import {
  type PipelineFieldName,
  resetPipelineFieldStates,
  validatePipelineFieldsForSubmit,
} from "../pipeline-submit"
import { navigate } from "../router"
import {
  PREVIEW_PAGE_SIZE,
  state,
  type PipelineIdentifierType,
} from "./state"
import type { TimezoneMode, TimestampFormat } from "../models/timestamp"

function parseDelimitedLine(line: string, delimiter: string): string[] {
  if (!delimiter) return [line]

  const cells: string[] = []
  let current = ""
  let inQuotes = false

  for (let i = 0; i < line.length; i++) {
    const char = line[i]
    if (char === '"') {
      if (inQuotes && line[i + 1] === '"') {
        current += '"'
        i++
      } else {
        inQuotes = !inQuotes
      }
      continue
    }

    if (!inQuotes && line.startsWith(delimiter, i)) {
      cells.push(current)
      current = ""
      i += delimiter.length - 1
      continue
    }

    current += char
  }

  cells.push(current)
  return cells
}

function normalizeHeaderName(value: string, index: number): string {
  return value.trim() || `Column ${index + 1}`
}

function preferredTimestampColumnIndex(headers: string[]): number {
  const preferredIndex = headers.findIndex((header) =>
    header.toLowerCase().includes("time")
  )
  return preferredIndex >= 0 ? preferredIndex : 0
}

function resolveTimestampColumnName(
  headers: string[],
  identifierType: PipelineIdentifierType,
  timestampKey: string
): string {
  if (headers.length === 0) return ""

  if (identifierType === "index") {
    const columnIndex = Number(timestampKey)
    if (
      Number.isInteger(columnIndex) &&
      columnIndex >= 1 &&
      columnIndex <= headers.length
    ) {
      return headers[columnIndex - 1]
    }
    return ""
  }

  return headers.includes(timestampKey) ? timestampKey : ""
}

export const parsedPreviewRows = computed(() => {
  if (!state.pipelinePreview) return []
  return state.pipelinePreview.raw_lines.map((line) =>
    parseDelimitedLine(line, state.pipelineForm.delimiter)
  )
})

export const previewHeaders = computed(() => {
  const rows = parsedPreviewRows.value
  const columnCount = rows.reduce((max, row) => Math.max(max, row.length), 0)

  if (!state.pipelineForm.hasHeaderRow) {
    const dataRows = rows.slice(Math.max(state.pipelineForm.dataStartRow - 1, 0))
    const count = (dataRows.length > 0 ? dataRows : rows).reduce(
      (max, row) => Math.max(max, row.length),
      0
    )
    return Array.from({ length: count }, (_, index) => `Column ${index + 1}`)
  }

  const headerRow = rows[state.pipelineForm.headerRow - 1] ?? []
  return Array.from({ length: columnCount }, (_, index) =>
    normalizeHeaderName(headerRow[index] ?? "", index)
  )
})

export const selectedPreviewTimestampColumn = computed(() =>
  resolveTimestampColumnName(
    previewHeaders.value,
    state.pipelineForm.identifierType,
    state.pipelineForm.timestamp.key
  )
)

function markPipelineField(
  field: PipelineFieldName,
  nextState: "idle" | "checking" | "valid" | "invalid",
  message?: string | null
): void {
  state.pipelineFieldStates[field] = {
    state: nextState,
    message: message ?? null,
  }
}

function invalidateValidatedPipeline(): void {
  state.pipelineReadyForMapping = false
  state.validatedPipelineSettings = null
  state.validatedColumnMappings = []
}

function validatePipelineForm(): boolean {
  resetPipelineFieldStates(state.pipelineFieldStates)

  return validatePipelineFieldsForSubmit({
    form: state.pipelineForm,
    hasPreview: state.pipelinePreview !== null,
    previewHeaders: previewHeaders.value,
    markField: markPipelineField,
  })
}

function refreshPipelineValidation(): void {
  if (!state.pipelineValidationAttempted) return

  validatePipelineForm()
}

function syncSelectionsWithPreview(): void {
  const headers = previewHeaders.value
  if (headers.length === 0) return

  const preferredIndex = preferredTimestampColumnIndex(headers)
  const preferredHeader = headers[preferredIndex]

  if (!state.pipelineForm.hasHeaderRow) {
    state.pipelineForm.identifierType = "index"
  }

  if (state.pipelineForm.identifierType === "index") {
    const currentIndex = Number(state.pipelineForm.timestamp.key)
    if (
      !Number.isInteger(currentIndex) ||
      currentIndex < 1 ||
      currentIndex > headers.length
    ) {
      state.pipelineForm.timestamp.key = String(preferredIndex + 1)
    }
    return
  }

  if (!headers.includes(state.pipelineForm.timestamp.key)) {
    state.pipelineForm.timestamp.key = preferredHeader
  }
}

export function canShowMorePreviewLines(): boolean {
  if (!state.pipelinePreview) return false
  return state.pipelinePreview.raw_lines.length < state.pipelinePreview.total_lines
}

export function updateHeaderRowFromPreview(lineNumber: number): void {
  invalidateValidatedPipeline()
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = lineNumber
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1
  }
  syncSelectionsWithPreview()
  refreshPipelineValidation()
}

export function updateDataStartRowFromPreview(lineNumber: number): void {
  invalidateValidatedPipeline()
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
  syncSelectionsWithPreview()
  refreshPipelineValidation()
}

export function applyPreviewLineSelection(lineNumber: number): void {
  if (state.pipelineSelectionTarget === "header-row") {
    updateHeaderRowFromPreview(lineNumber)
    state.pipelineSelectionTarget = null
  } else if (state.pipelineSelectionTarget === "data-start-row") {
    updateDataStartRowFromPreview(lineNumber)
    state.pipelineSelectionTarget = null
  }
}

export function applyPreviewColumnSelection(columnName: string): void {
  if (
    state.pipelineSelectionTarget &&
    state.pipelineSelectionTarget !== "timestamp-column"
  ) {
    return
  }

  invalidateValidatedPipeline()
  state.pipelineForm.timestamp.key =
    state.pipelineForm.identifierType === "index"
      ? String(previewHeaders.value.indexOf(columnName) + 1)
      : columnName
  state.pipelineSelectionTarget = null
  refreshPipelineValidation()
}

export function setPipelineHasHeaderRow(enabled: boolean): void {
  invalidateValidatedPipeline()
  const headersBeforeToggle = previewHeaders.value
  const currentVisibleTimestampColumn = resolveTimestampColumnName(
    headersBeforeToggle,
    state.pipelineForm.identifierType,
    state.pipelineForm.timestamp.key
  )

  state.pipelineForm.hasHeaderRow = enabled
  if (!enabled && state.pipelineSelectionTarget === "header-row") {
    state.pipelineSelectionTarget = null
  }

  if (!enabled) {
    state.pipelineForm.identifierType = "index"
    if (currentVisibleTimestampColumn) {
      const currentIndex = headersBeforeToggle.indexOf(currentVisibleTimestampColumn)
      if (currentIndex >= 0) {
        state.pipelineForm.timestamp.key = String(currentIndex + 1)
      }
    }
    state.pipelineForm.dataStartRow = Math.max(1, state.pipelineForm.dataStartRow)
  } else {
    state.pipelineForm.dataStartRow = Math.max(2, state.pipelineForm.dataStartRow)
    if (state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow) {
      state.pipelineForm.headerRow = Math.max(
        1,
        state.pipelineForm.dataStartRow - 1
      )
    }
  }

  syncSelectionsWithPreview()
  refreshPipelineValidation()
}

export function setPipelineIdentifierType(identifierType: PipelineIdentifierType): void {
  invalidateValidatedPipeline()

  if (!state.pipelineForm.hasHeaderRow && identifierType === "name") {
    return
  }

  const headers = previewHeaders.value
  const currentVisibleTimestampColumn = resolveTimestampColumnName(
    headers,
    state.pipelineForm.identifierType,
    state.pipelineForm.timestamp.key
  )

  state.pipelineForm.identifierType = identifierType

  if (headers.length === 0) {
    state.pipelineForm.timestamp.key =
      identifierType === "index" ? "1" : "timestamp"
    refreshPipelineValidation()
    return
  }

  if (identifierType === "index") {
    const currentIndex = headers.indexOf(currentVisibleTimestampColumn)
    state.pipelineForm.timestamp.key =
      currentIndex >= 0
        ? String(currentIndex + 1)
        : String(preferredTimestampColumnIndex(headers) + 1)
  } else {
    state.pipelineForm.timestamp.key =
      currentVisibleTimestampColumn || headers[preferredTimestampColumnIndex(headers)]
  }

  refreshPipelineValidation()
}

function syncTimestampFormat(format: TimestampFormat): void {
  const timestamp = state.pipelineForm.timestamp
  timestamp.format = format

  if (format === "custom") {
    timestamp.customFormat = timestamp.customFormat || "%Y-%m-%d %H:%M:%S"
  } else {
    timestamp.customFormat = undefined
  }

  if (format === "ISO8601") {
    syncTimestampTimezone("embeddedOffset")
  } else {
    syncTimestampTimezone("utc")
  }
}

function syncTimestampTimezone(mode: TimezoneMode): void {
  const timestamp = state.pipelineForm.timestamp
  timestamp.timezoneMode = mode

  if (mode === "utc" || mode === "embeddedOffset") {
    timestamp.timezone = undefined
  } else if (mode === "fixedOffset") {
    timestamp.timezone = "-0700"
  } else if (mode === "daylightSavings") {
    timestamp.timezone = "America/Denver"
  }
}

export function updatePipelineField(name: string, value: string): void {
  invalidateValidatedPipeline()

  switch (name) {
    case "file_path":
      state.pipelineForm.filePath = value
      state.pipelinePreview = null
      state.pipelineSelectionTarget = null
      state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
      break
    case "header_row":
      state.pipelineForm.headerRow = Math.max(1, Number(value) || 1)
      if (
        state.pipelineForm.hasHeaderRow &&
        state.pipelineForm.dataStartRow <= state.pipelineForm.headerRow
      ) {
        state.pipelineForm.dataStartRow = state.pipelineForm.headerRow + 1
      }
      syncSelectionsWithPreview()
      break
    case "data_start_row":
      state.pipelineForm.dataStartRow = Math.max(
        state.pipelineForm.hasHeaderRow ? 2 : 1,
        Number(value) || 1
      )
      if (
        state.pipelineForm.hasHeaderRow &&
        state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow
      ) {
        state.pipelineForm.headerRow = state.pipelineForm.dataStartRow - 1
      }
      syncSelectionsWithPreview()
      break
    case "delimiter":
      state.pipelineForm.delimiter = value || ","
      syncSelectionsWithPreview()
      break
    case "timestamp_key":
      state.pipelineForm.timestamp.key = value
      syncSelectionsWithPreview()
      break
    case "timestamp_format":
      if (value === "ISO8601" || value === "naive" || value === "custom") {
        syncTimestampFormat(value)
      }
      break
    case "custom_timestamp_format":
      state.pipelineForm.timestamp.customFormat = value
      break
    case "timezone_mode":
      if (
        value === "embeddedOffset" ||
        value === "utc" ||
        value === "fixedOffset" ||
        value === "daylightSavings"
      ) {
        syncTimestampTimezone(value)
      }
      break
    case "timezone":
      state.pipelineForm.timestamp.timezone = value
      break
  }

  refreshPipelineValidation()
}

export async function loadPipelinePreview(
  path: string,
  rows = PREVIEW_PAGE_SIZE
): Promise<void> {
  invalidateValidatedPipeline()

  if (!path.trim()) {
    return
  }

  try {
    const preview = await getCsvPreview(path.trim(), rows)
    const shouldApplyDetectedDefaults =
      !state.pipelinePreview || state.pipelineForm.filePath !== path.trim()

    state.pipelinePreview = preview
    state.pipelineForm.filePath = path.trim()

    if (shouldApplyDetectedDefaults) {
      state.pipelineForm.hasHeaderRow = preview.detected_header_row !== null
      state.pipelineForm.headerRow =
        preview.detected_header_row ?? state.pipelineForm.headerRow
      state.pipelineForm.dataStartRow =
        preview.detected_data_start_row ?? state.pipelineForm.dataStartRow
      state.pipelineForm.delimiter =
        preview.detected_delimiter || state.pipelineForm.delimiter
      state.pipelineForm.identifierType = state.pipelineForm.hasHeaderRow
        ? "name"
        : "index"
    }

    state.pipelineSelectionTarget = null
    syncSelectionsWithPreview()
    state.pipelinePreviewRowsRequested = rows
    refreshPipelineValidation()
  } catch {
    state.pipelinePreview = null
    state.pipelineSelectionTarget = null
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
    refreshPipelineValidation()
  }
}

export async function showMorePreviewLines(): Promise<void> {
  if (!state.pipelinePreview) return

  const nextRows = Math.min(
    state.pipelinePreviewRowsRequested + PREVIEW_PAGE_SIZE,
    state.pipelinePreview.total_lines
  )
  await loadPipelinePreview(state.pipelineForm.filePath, nextRows)
}

export async function browseForCsvPath(): Promise<void> {
  try {
    const dialog = await import("@tauri-apps/plugin-dialog")
    const selection = await dialog.open({
      directory: false,
      multiple: false,
      filters: [{ name: "CSV files", extensions: ["csv", "txt"] }],
    })

    if (typeof selection !== "string" || !selection) return

    updatePipelineField("file_path", selection)
    await loadPipelinePreview(selection)
  } catch {
    return
  }
}

export function submitPipelineConfig(): void {
  state.pipelineValidationAttempted = true

  const valid = validatePipelineForm()
  if (!valid) {
    invalidateValidatedPipeline()
    return
  }

  state.validatedPipelineSettings = buildPipelineTransformerSettings()
  state.pipelineReadyForMapping = true
  navigate("jobs-new-mapping")
}

export function buildPipelineTransformerSettings() {
  const settings: CsvTransformerSettings = {
    headerRow:
      state.pipelineForm.hasHeaderRow && state.pipelineForm.identifierType === "name"
        ? state.pipelineForm.headerRow
        : null,
    dataStartRow: state.pipelineForm.dataStartRow,
    delimiter: state.pipelineForm.delimiter,
    identifierType: state.pipelineForm.identifierType,
    timestamp: {
      ...state.pipelineForm.timestamp,
    },
  }

  if (settings.timestamp.format !== "custom") {
    delete settings.timestamp.customFormat
  }

  if (
    settings.timestamp.timezoneMode !== "fixedOffset" &&
    settings.timestamp.timezoneMode !== "daylightSavings"
  ) {
    delete settings.timestamp.timezone
  }

  return settings
}
