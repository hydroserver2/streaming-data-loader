import test from "node:test"
import assert from "node:assert/strict"

import type { CsvPreviewResponse } from "./api"
import {
  applyPreviewColumnSelection,
  loadPipelinePreview,
  selectedPreviewTimestampColumn,
  setPipelineHasHeaderRow,
  showMorePreviewLines,
  updatePipelineField,
} from "./composables/usePipeline"
import {
  createEmptyPipelineForm,
  PREVIEW_PAGE_SIZE,
  state,
} from "./composables/state"

const originalFetch = globalThis.fetch

function createPreview(
  overrides: Partial<CsvPreviewResponse> = {}
): CsvPreviewResponse {
  return {
    raw_lines: [
      "recorded_at,value",
      "2024-01-01T00:00:00Z,1.2",
    ],
    parsed_rows: [
      ["recorded_at", "value"],
      ["2024-01-01T00:00:00Z", "1.2"],
    ],
    detected_header_row: 1,
    detected_data_start_row: 2,
    detected_delimiter: ",",
    total_lines: 2,
    encoding: "utf-8",
    ...overrides,
  }
}

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  })
}

function resetPipelineState(): void {
  state.pipelineForm = createEmptyPipelineForm()
  state.pipelinePreview = null
  state.pipelineFeedback = null
  state.pipelineSelectionTarget = null
  state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
}

test.beforeEach(() => {
  resetPipelineState()
  globalThis.fetch = originalFetch
})

test.after(() => {
  globalThis.fetch = originalFetch
})

test("disabling the header row forces index mode and preserves the timestamp selection", () => {
  state.pipelinePreview = createPreview()
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.identifierType = "name"
  state.pipelineForm.timestampKey = "recorded_at"

  setPipelineHasHeaderRow(false)

  assert.equal(state.pipelineForm.hasHeaderRow, false)
  assert.equal(state.pipelineForm.identifierType, "index")
  assert.equal(state.pipelineForm.timestampKey, "1")
})

test("preview column selection stores a 1-based index in index mode", () => {
  state.pipelinePreview = createPreview()
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.identifierType = "index"
  state.pipelineForm.timestampKey = "1"

  applyPreviewColumnSelection("value")

  assert.equal(state.pipelineForm.timestampKey, "2")
  assert.equal(selectedPreviewTimestampColumn.value, "value")
})

test("loading more preview rows does not overwrite manual transformer fixes", async () => {
  const responses = [
    createPreview({
      raw_lines: [
        "recorded_at,value",
        "2024-01-01T00:00:00Z,1.2",
      ],
      total_lines: 4,
    }),
    createPreview({
      raw_lines: [
        "recorded_at,value",
        "2024-01-01T00:00:00Z,1.2",
        "2024-01-02T00:00:00Z,1.4",
        "2024-01-03T00:00:00Z,1.6",
      ],
      total_lines: 4,
    }),
  ]

  let callCount = 0
  globalThis.fetch = async () => jsonResponse(responses[callCount++])

  await loadPipelinePreview("/tmp/preview.csv")
  updatePipelineField("delimiter", ";")
  await showMorePreviewLines()

  assert.equal(callCount, 2)
  assert.equal(state.pipelineForm.delimiter, ";")
  assert.equal(state.pipelinePreview?.raw_lines.length, 4)
})
