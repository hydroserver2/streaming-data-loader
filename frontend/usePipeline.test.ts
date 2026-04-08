import test from "node:test"
import assert from "node:assert/strict"

import type { CsvPreviewResponse } from "./api"
import {
  applyPreviewColumnSelection,
  buildPipelineTransformerSettings,
  loadPipelinePreview,
  selectedPreviewTimestampColumn,
  setPipelineHasHeaderRow,
  showMorePreviewLines,
  submitPipelineConfig,
  updatePipelineField,
} from "./composables/usePipeline"
import {
  createEmptyPipelineForm,
  PREVIEW_PAGE_SIZE,
  state,
} from "./composables/state"
import { createPipelineFieldStates } from "./pipeline-submit"

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
  state.pipelineFieldStates = createPipelineFieldStates()
  state.pipelineValidationAttempted = false
  state.pipelineReadyForMapping = false
  state.validatedPipelineSettings = null
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
  state.pipelineForm.timestamp.key = "recorded_at"

  setPipelineHasHeaderRow(false)

  assert.equal(state.pipelineForm.hasHeaderRow, false)
  assert.equal(state.pipelineForm.identifierType, "index")
  assert.equal(state.pipelineForm.timestamp.key, "1")
})

test("preview column selection stores a 1-based index in index mode", () => {
  state.pipelinePreview = createPreview()
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.identifierType = "index"
  state.pipelineForm.timestamp.key = "1"

  applyPreviewColumnSelection("value")

  assert.equal(state.pipelineForm.timestamp.key, "2")
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

test("custom timestamp formats default to UTC timezone handling", () => {
  assert.equal(state.pipelineForm.timestamp.format, "ISO8601")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "embeddedOffset")

  updatePipelineField("timestamp_format", "custom")

  assert.equal(state.pipelineForm.timestamp.format, "custom")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "utc")
  assert.equal(
    state.pipelineForm.timestamp.customFormat,
    "%Y-%m-%d %H:%M:%S"
  )
})

test("selecting daylight-savings mode defaults to an IANA timezone", () => {
  updatePipelineField("timestamp_format", "naive")
  updatePipelineField("timezone_mode", "daylightSavings")

  assert.equal(state.pipelineForm.timestamp.format, "naive")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "daylightSavings")
  assert.equal(state.pipelineForm.timestamp.timezone, "America/Denver")
})

test("switching timezone modes resets the controlled vocabulary selection", () => {
  updatePipelineField("timestamp_format", "naive")
  updatePipelineField("timezone_mode", "fixedOffset")
  updatePipelineField("timezone", "-0600")
  updatePipelineField("timezone_mode", "daylightSavings")

  assert.equal(state.pipelineForm.timestamp.timezoneMode, "daylightSavings")
  assert.equal(state.pipelineForm.timestamp.timezone, "America/Denver")
})

test("serializing the pipeline form matches hydroserver csv transformer settings", () => {
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.delimiter = "|"
  state.pipelineForm.identifierType = "name"
  state.pipelineForm.timestamp = {
    key: "recorded_at",
    format: "custom",
    customFormat: "%m/%d/%Y %H:%M:%S",
    timezoneMode: "daylightSavings",
    timezone: "America/Denver",
  }

  assert.deepEqual(buildPipelineTransformerSettings(), {
    headerRow: 1,
    dataStartRow: 2,
    delimiter: "|",
    identifierType: "name",
    timestamp: {
      key: "recorded_at",
      format: "custom",
      customFormat: "%m/%d/%Y %H:%M:%S",
      timezoneMode: "daylightSavings",
      timezone: "America/Denver",
    },
  })
})

test("serializing index mode clears headerRow so hydroserverpy skips file headers", () => {
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.identifierType = "index"
  state.pipelineForm.timestamp.key = "1"

  assert.deepEqual(buildPipelineTransformerSettings(), {
    headerRow: null,
    dataStartRow: 2,
    delimiter: ",",
    identifierType: "index",
    timestamp: {
      key: "1",
      format: "ISO8601",
      timezoneMode: "embeddedOffset",
    },
  })
})

test("submitPipelineConfig marks the transformer as ready for mapping when validation passes", () => {
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/preview.csv"
  state.pipelineForm.hasHeaderRow = true
  state.pipelineForm.headerRow = 1
  state.pipelineForm.dataStartRow = 2
  state.pipelineForm.identifierType = "name"
  state.pipelineForm.timestamp.key = "recorded_at"

  submitPipelineConfig()

  assert.equal(state.pipelineReadyForMapping, true)
  assert.deepEqual(state.validatedPipelineSettings, {
    headerRow: 1,
    dataStartRow: 2,
    delimiter: ",",
    identifierType: "name",
    timestamp: {
      key: "recorded_at",
      format: "ISO8601",
      timezoneMode: "embeddedOffset",
    },
  })
  assert.equal(state.pipelineFeedback?.tone, "success")
})

test("changing the form after a submit attempt revalidates and clears mapping readiness", () => {
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/preview.csv"
  state.pipelineForm.timestamp.key = "recorded_at"

  submitPipelineConfig()
  updatePipelineField("timestamp_format", "custom")
  updatePipelineField("custom_timestamp_format", "")

  assert.equal(state.pipelineReadyForMapping, false)
  assert.equal(state.validatedPipelineSettings, null)
  assert.equal(
    state.pipelineFieldStates.custom_timestamp_format.state,
    "invalid"
  )
})
