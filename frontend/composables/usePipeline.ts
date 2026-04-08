import { computed } from "vue"

import { getCsvPreview } from "../api"
import {
  PREVIEW_PAGE_SIZE,
  state,
  type PipelineIdentifierType,
} from "./state"

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
    state.pipelineForm.timestampKey
  )
)

function syncSelectionsWithPreview(): void {
  const headers = previewHeaders.value
  if (headers.length === 0) return

  const preferredIndex = preferredTimestampColumnIndex(headers)
  const preferredHeader = headers[preferredIndex]

  if (!state.pipelineForm.hasHeaderRow) {
    state.pipelineForm.identifierType = "index"
  }

  if (state.pipelineForm.identifierType === "index") {
    const currentIndex = Number(state.pipelineForm.timestampKey)
    if (
      !Number.isInteger(currentIndex) ||
      currentIndex < 1 ||
      currentIndex > headers.length
    ) {
      state.pipelineForm.timestampKey = String(preferredIndex + 1)
    }
    return
  }

  if (!headers.includes(state.pipelineForm.timestampKey)) {
    state.pipelineForm.timestampKey = preferredHeader
  }
}

export function canShowMorePreviewLines(): boolean {
  if (!state.pipelinePreview) return false
  return state.pipelinePreview.raw_lines.length < state.pipelinePreview.total_lines
}

export function updateHeaderRowFromPreview(lineNumber: number): void {
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = lineNumber
  if (state.pipelineForm.dataStartRow <= lineNumber) {
    state.pipelineForm.dataStartRow = lineNumber + 1
  }
  syncSelectionsWithPreview()
}

export function updateDataStartRowFromPreview(lineNumber: number): void {
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

  state.pipelineForm.timestampKey =
    state.pipelineForm.identifierType === "index"
      ? String(previewHeaders.value.indexOf(columnName) + 1)
      : columnName
  state.pipelineSelectionTarget = null
}

export function setPipelineHasHeaderRow(enabled: boolean): void {
  const headersBeforeToggle = previewHeaders.value
  const currentVisibleTimestampColumn = resolveTimestampColumnName(
    headersBeforeToggle,
    state.pipelineForm.identifierType,
    state.pipelineForm.timestampKey
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
        state.pipelineForm.timestampKey = String(currentIndex + 1)
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
}

export function setPipelineIdentifierType(identifierType: PipelineIdentifierType): void {
  state.pipelineFeedback = null

  if (!state.pipelineForm.hasHeaderRow && identifierType === "name") {
    return
  }

  const headers = previewHeaders.value
  const currentVisibleTimestampColumn = resolveTimestampColumnName(
    headers,
    state.pipelineForm.identifierType,
    state.pipelineForm.timestampKey
  )

  state.pipelineForm.identifierType = identifierType

  if (headers.length === 0) {
    state.pipelineForm.timestampKey = identifierType === "index" ? "1" : "timestamp"
    return
  }

  if (identifierType === "index") {
    const currentIndex = headers.indexOf(currentVisibleTimestampColumn)
    state.pipelineForm.timestampKey =
      currentIndex >= 0
        ? String(currentIndex + 1)
        : String(preferredTimestampColumnIndex(headers) + 1)
  } else {
    state.pipelineForm.timestampKey =
      currentVisibleTimestampColumn || headers[preferredTimestampColumnIndex(headers)]
  }
}

export function updatePipelineField(name: string, value: string): void {
  state.pipelineFeedback = null

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
      state.pipelineForm.timestampKey = value
      syncSelectionsWithPreview()
      break
    case "timestamp_type":
      state.pipelineForm.timestampType = value === "custom" ? "custom" : "iso"
      if (state.pipelineForm.timestampType !== "custom") {
        state.pipelineForm.timestampFormat = ""
      } else if (!state.pipelineForm.timestampFormat.trim()) {
        state.pipelineForm.timestampFormat = "%Y-%m-%d %H:%M:%S"
      }
      break
    case "timestamp_format":
      state.pipelineForm.timestampFormat = value
      break
    case "timezone_type":
      state.pipelineForm.timezoneType =
        value === "utc" || value === "offset" || value === "iana" ? value : ""
      if (
        state.pipelineForm.timezoneType !== "offset" &&
        state.pipelineForm.timezoneType !== "iana"
      ) {
        state.pipelineForm.timezone = ""
      }
      break
    case "timezone":
      state.pipelineForm.timezone = value
      break
  }
}

export async function loadPipelinePreview(
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
    state.pipelineFeedback = null
  } catch (error) {
    state.pipelinePreview = null
    state.pipelineSelectionTarget = null
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't preview that CSV file.",
    }
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
    state.pipelineFeedback = {
      tone: "info",
      message:
        "The native file picker is only available in the desktop app. Enter the CSV path manually.",
    }
  }
}
