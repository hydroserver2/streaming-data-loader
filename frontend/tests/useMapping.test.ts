import test from "node:test"
import assert from "node:assert/strict"

import type { CsvPreviewResponse, DatastreamSummary } from "../api/hydroserver"
import { createPipelineFieldStates } from "../pipeline-submit"
import {
  buildPipelineColumnMappings,
  buildDatastreamBrowserEntries,
  buildMappingSourceColumns,
  loadPipelineDatastreams,
  pipelineThingOptions,
  syncPipelineMappingDrafts,
  updatePipelineMappingDatastream,
  updatePipelineMappingThing,
} from "../composables/useMapping"
import { createEmptyPipelineForm, PREVIEW_PAGE_SIZE, state } from "../composables/state"

const originalFetch = globalThis.fetch

function createPreview(): CsvPreviewResponse {
  return {
    raw_lines: [
      "recorded_at,stage_cfs,temp_c",
      "2024-01-01T00:00:00Z,1.2,7.4",
    ],
    parsed_rows: [
      ["recorded_at", "stage_cfs", "temp_c"],
      ["2024-01-01T00:00:00Z", "1.2", "7.4"],
    ],
    detected_header_row: 1,
    detected_data_start_row: 2,
    detected_delimiter: ",",
    total_lines: 2,
    encoding: "utf-8",
  }
}

function datastream(
  overrides: Partial<DatastreamSummary>
): DatastreamSummary {
  return {
    id: "stream-1",
    name: "Stage Datastream",
    thing_id: "thing-1",
    thing_name: "Alpha Site",
    observed_property_name: "Stage",
    processing_level_definition: "Raw",
    unit_name: "cubic feet per second",
    unit_symbol: "cfs",
    sampled_medium: "Surface water",
    sensor_name: "Pressure transducer",
    result_type: "Measure",
    ...overrides,
  }
}

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  })
}

function resetMappingState(): void {
  state.pipelineForm = createEmptyPipelineForm()
  state.pipelinePreview = createPreview()
  state.pipelineSelectionTarget = null
  state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
  state.pipelineFieldStates = createPipelineFieldStates()
  state.pipelineValidationAttempted = false
  state.pipelineReadyForMapping = true
  state.validatedPipelineSettings = {
    headerRow: 1,
    dataStartRow: 2,
    delimiter: ",",
    identifierType: "name",
    timestamp: {
      key: "recorded_at",
      format: "ISO8601",
      timezoneMode: "embeddedOffset",
    },
  }
  state.pipelineDatastreams = []
  state.pipelineDatastreamsLoading = false
  state.pipelineMappingDrafts = []
  state.validatedColumnMappings = []
}

test.beforeEach(() => {
  resetMappingState()
  globalThis.fetch = originalFetch
})

test.after(() => {
  globalThis.fetch = originalFetch
})

test("buildMappingSourceColumns excludes the timestamp column and uses 1-based keys in index mode", () => {
  assert.deepEqual(
    buildMappingSourceColumns(["Timestamp", "Stage", "Temperature"], "index", "1"),
    [
      { csvColumn: "2", label: "2 · Stage" },
      { csvColumn: "3", label: "3 · Temperature" },
    ]
  )
})

test("changing the selected thing clears a datastream from a different thing", () => {
  state.pipelineDatastreams = [
    datastream({}),
    datastream({
      id: "stream-2",
      name: "Temperature Datastream",
      thing_id: "thing-2",
      thing_name: "Beta Site",
      observed_property_name: "Temperature",
      unit_name: "degree Celsius",
      unit_symbol: "degC",
    }),
  ]

  syncPipelineMappingDrafts()
  updatePipelineMappingDatastream("stage_cfs", "stream-1")
  updatePipelineMappingThing("stage_cfs", "thing-2")

  assert.equal(
    state.pipelineMappingDrafts.find((draft) => draft.csvColumn === "stage_cfs")
      ?.datastreamId,
    ""
  )
})

test("selecting a datastream that is already mapped leaves the existing mapping in place", () => {
  state.pipelineDatastreams = [
    datastream({}),
    datastream({
      id: "stream-2",
      name: "Temperature Datastream",
      observed_property_name: "Temperature",
      unit_name: "degree Celsius",
      unit_symbol: "degC",
    }),
  ]

  syncPipelineMappingDrafts()
  updatePipelineMappingDatastream("stage_cfs", "stream-1")
  updatePipelineMappingDatastream("temp_c", "stream-1")

  assert.equal(
    state.pipelineMappingDrafts.find((draft) => draft.csvColumn === "stage_cfs")
      ?.datastreamId,
    "stream-1"
  )
  assert.equal(
    state.pipelineMappingDrafts.find((draft) => draft.csvColumn === "temp_c")
      ?.datastreamId,
    ""
  )
})

test("buildPipelineColumnMappings uses the selected datastream names", () => {
  state.pipelineDatastreams = [
    datastream({}),
    datastream({
      id: "stream-2",
      name: "Temperature Datastream",
      thing_id: "thing-2",
      thing_name: "Beta Site",
      observed_property_name: "Temperature",
      unit_name: "degree Celsius",
      unit_symbol: "degC",
    }),
  ]

  syncPipelineMappingDrafts()
  updatePipelineMappingDatastream("stage_cfs", "stream-1")
  updatePipelineMappingDatastream("temp_c", "stream-2")

  assert.deepEqual(buildPipelineColumnMappings(), [
    {
      csv_column: "stage_cfs",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
    {
      csv_column: "temp_c",
      datastream_id: "stream-2",
      datastream_name: "Temperature Datastream",
    },
  ])
  assert.equal(state.validatedColumnMappings.length, 2)
})

test("loadPipelineDatastreams sorts thing options by thing name", async () => {
  globalThis.fetch = async () =>
    jsonResponse([
      datastream({
        id: "stream-2",
        thing_id: "thing-2",
        thing_name: "Zulu Site",
      }),
      datastream({
        id: "stream-1",
        thing_id: "thing-1",
        thing_name: "Alpha Site",
      }),
      datastream({
        id: "stream-3",
        thing_id: "thing-1",
        thing_name: "Alpha Site",
        observed_property_name: "Temperature",
      }),
    ])

  await loadPipelineDatastreams(true)

  assert.deepEqual(pipelineThingOptions.value, [
    { id: "thing-1", name: "Alpha Site" },
    { id: "thing-2", name: "Zulu Site" },
  ])
})

test("buildDatastreamBrowserEntries groups datastreams by thing name and includes mapped labels", () => {
  const entries = buildDatastreamBrowserEntries(
    [
      datastream({
        id: "stream-2",
        thing_id: "thing-2",
        thing_name: "Zulu Site",
        observed_property_name: "Temperature",
      }),
      datastream({
        id: "stream-1",
        thing_id: "thing-1",
        thing_name: "Alpha Site",
      }),
    ],
    [{ csvColumn: "stage_cfs", thingId: "thing-1", datastreamId: "stream-1" }],
    [
      { csvColumn: "stage_cfs", label: "stage_cfs" },
      { csvColumn: "temp_c", label: "temp_c" },
    ]
  )

  assert.deepEqual(entries, [
    {
      kind: "thing",
      key: "thing-thing-1",
      thingId: "thing-1",
      thingName: "Alpha Site",
    },
    {
      kind: "datastream",
      key: "datastream-stream-1",
      datastream: datastream({
        id: "stream-1",
        thing_id: "thing-1",
        thing_name: "Alpha Site",
      }),
      mappedCsvColumn: "stage_cfs",
      mappedColumnLabel: "stage_cfs",
    },
    {
      kind: "thing",
      key: "thing-thing-2",
      thingId: "thing-2",
      thingName: "Zulu Site",
    },
    {
      kind: "datastream",
      key: "datastream-stream-2",
      datastream: datastream({
        id: "stream-2",
        thing_id: "thing-2",
        thing_name: "Zulu Site",
        observed_property_name: "Temperature",
      }),
      mappedCsvColumn: null,
      mappedColumnLabel: null,
    },
  ])
})
