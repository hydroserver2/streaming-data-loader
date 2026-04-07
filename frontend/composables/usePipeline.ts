import { computed } from "vue"

import { getCsvPreview } from "../api"
import {
  PREVIEW_PAGE_SIZE,
  state,
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

function syncSelectionsWithPreview(): void {
  const headers = previewHeaders.value
  if (headers.length === 0) return

  const preferredHeader =
    headers.find((header) => header.toLowerCase().includes("time")) ?? headers[0]

  if (!headers.includes(state.pipelineForm.timestampColumn)) {
    state.pipelineForm.timestampColumn = preferredHeader
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

  state.pipelineForm.timestampColumn = columnName
  state.pipelineSelectionTarget = null
}

export function setPipelineHasHeaderRow(enabled: boolean): void {
  state.pipelineForm.hasHeaderRow = enabled
  if (!enabled && state.pipelineSelectionTarget === "header-row") {
    state.pipelineSelectionTarget = null
  }
  if (
    enabled &&
    state.pipelineForm.headerRow >= state.pipelineForm.dataStartRow
  ) {
    state.pipelineForm.headerRow = Math.max(1, state.pipelineForm.dataStartRow - 1)
  }
  syncSelectionsWithPreview()
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
      state.pipelineForm.headerRow = Number(value) || 1
      syncSelectionsWithPreview()
      break
    case "data_start_row":
      state.pipelineForm.dataStartRow = Number(value) || 1
      syncSelectionsWithPreview()
      break
    case "delimiter":
      state.pipelineForm.delimiter = value || ","
      syncSelectionsWithPreview()
      break
    case "timestamp_column":
      state.pipelineForm.timestampColumn = value
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
    state.pipelinePreview = preview
    state.pipelineForm.filePath = path.trim()
    state.pipelineForm.hasHeaderRow = preview.detected_header_row !== null
    state.pipelineForm.headerRow =
      preview.detected_header_row ?? state.pipelineForm.headerRow
    state.pipelineForm.dataStartRow =
      preview.detected_data_start_row ?? state.pipelineForm.dataStartRow
    state.pipelineForm.delimiter =
      preview.detected_delimiter || state.pipelineForm.delimiter
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
