import test from "node:test"
import assert from "node:assert/strict"

import type { CsvPreviewResponse } from "../api/hydroserver"
import {
  editPipelineCsvSetup,
  editPipelineMappings,
  editPipelineSourceFile,
  abandonPipelineCreation,
  applyPreviewColumnSelection,
  buildPipelineTransformerSettings,
  createPipelineDatasource,
  loadPipelinePreview,
  selectedPreviewTimestampColumn,
  setPipelineHasHeaderRow,
  showMorePreviewLines,
  submitPipelineConfig,
  updatePipelineField,
} from "../composables/usePipeline"
import {
  createEmptyPipelineForm,
  PREVIEW_PAGE_INCREMENT,
  PREVIEW_PAGE_SIZE,
  state,
} from "../composables/state"
import { createPipelineFieldStates } from "../pipeline-submit"

const originalFetch = globalThis.fetch
const originalWindow = globalThis.window

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
  state.pipelineSelectionTarget = null
  state.pipelineEditorStartStep = null
  state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
  state.pipelineFieldStates = createPipelineFieldStates()
  state.pipelineValidationAttempted = false
  state.pipelineReadyForMapping = false
  state.validatedPipelineSettings = null
  state.pipelineDatastreams = []
  state.pipelineDatastreamsLoading = false
  state.pipelineMappingDrafts = []
  state.validatedColumnMappings = []
  state.pipelineEditTarget = null
  state.connectionSummary = null
  state.lastConnectionState = null
  state.config = null
  state.pipelineCreateSubmitting = false
}

test.beforeEach(() => {
  resetPipelineState()
  globalThis.fetch = originalFetch
})

test.after(() => {
  globalThis.fetch = originalFetch
  if (originalWindow === undefined) {
    Reflect.deleteProperty(globalThis, "window")
  } else {
    Object.defineProperty(globalThis, "window", {
      value: originalWindow,
      configurable: true,
      writable: true,
    })
  }
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
      raw_lines: Array.from(
        { length: PREVIEW_PAGE_SIZE },
        (_, index) =>
          index === 0
            ? "recorded_at,value"
            : `2024-01-${String(index).padStart(2, "0")}T00:00:00Z,${index}`
      ),
      parsed_rows: Array.from(
        { length: PREVIEW_PAGE_SIZE },
        (_, index) =>
          index === 0
            ? ["recorded_at", "value"]
            : [`2024-01-${String(index).padStart(2, "0")}T00:00:00Z`, String(index)]
      ),
      total_lines: PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT,
    }),
    createPreview({
      raw_lines: Array.from(
        { length: PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT },
        (_, index) =>
          index === 0
            ? "recorded_at,value"
            : `2024-01-${String(index).padStart(2, "0")}T00:00:00Z,${index}`
      ),
      parsed_rows: Array.from(
        { length: PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT },
        (_, index) =>
          index === 0
            ? ["recorded_at", "value"]
            : [`2024-01-${String(index).padStart(2, "0")}T00:00:00Z`, String(index)]
      ),
      total_lines: PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT,
    }),
  ]

  let callCount = 0
  globalThis.fetch = async () => jsonResponse(responses[callCount++])

  await loadPipelinePreview("/tmp/preview.csv")
  updatePipelineField("delimiter", ";")
  await showMorePreviewLines()

  assert.equal(callCount, 2)
  assert.equal(state.pipelineForm.delimiter, ";")
  assert.equal(
    state.pipelinePreviewRowsRequested,
    PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT
  )
  assert.equal(
    state.pipelinePreview?.raw_lines.length,
    PREVIEW_PAGE_SIZE + PREVIEW_PAGE_INCREMENT
  )
})

test("loading a preview auto-detects an ISO timestamp column and timestamp settings", async () => {
  globalThis.fetch = async () =>
    jsonResponse(
      createPreview({
        raw_lines: [
          "value,recorded_at,status",
          "1.2,2024-01-01T00:00:00Z,ok",
          "1.4,2024-01-02T00:00:00Z,ok",
        ],
        parsed_rows: [
          ["value", "recorded_at", "status"],
          ["1.2", "2024-01-01T00:00:00Z", "ok"],
          ["1.4", "2024-01-02T00:00:00Z", "ok"],
        ],
        total_lines: 3,
      })
    )

  await loadPipelinePreview("/tmp/iso-preview.csv")

  assert.equal(state.pipelineForm.identifierType, "name")
  assert.equal(state.pipelineForm.timestamp.key, "recorded_at")
  assert.equal(state.pipelineForm.timestamp.format, "ISO8601")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "embeddedOffset")
})

test("loading a preview auto-detects a custom timestamp format", async () => {
  globalThis.fetch = async () =>
    jsonResponse(
      createPreview({
        raw_lines: [
          "timestamp,value",
          "04/07/2026 13:45:00,1.2",
          "04/07/2026 13:50:00,1.3",
        ],
        parsed_rows: [
          ["timestamp", "value"],
          ["04/07/2026 13:45:00", "1.2"],
          ["04/07/2026 13:50:00", "1.3"],
        ],
        total_lines: 3,
      })
    )

  await loadPipelinePreview("/tmp/custom-preview.csv")

  assert.equal(state.pipelineForm.timestamp.key, "timestamp")
  assert.equal(state.pipelineForm.timestamp.format, "custom")
  assert.equal(state.pipelineForm.timestamp.customFormat, "%m/%d/%Y %H:%M:%S")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "utc")
})

test("loading a preview falls back to the first column when no timestamp column is detectable", async () => {
  globalThis.fetch = async () =>
    jsonResponse(
      createPreview({
        raw_lines: [
          "sensor,value,status",
          "alpha,1.2,ok",
          "beta,1.4,ok",
        ],
        parsed_rows: [
          ["sensor", "value", "status"],
          ["alpha", "1.2", "ok"],
          ["beta", "1.4", "ok"],
        ],
        total_lines: 3,
      })
    )

  await loadPipelinePreview("/tmp/fallback-preview.csv")

  assert.equal(state.pipelineForm.timestamp.key, "sensor")
  assert.equal(state.pipelineForm.timestamp.format, "ISO8601")
  assert.equal(state.pipelineForm.timestamp.timezoneMode, "embeddedOffset")
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

test("createPipelineDatasource sends the expected payload and resets the wizard after success", async () => {
  let requestBody: Record<string, unknown> | null = null

  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#jobs/new/mapping" } },
    configurable: true,
    writable: true,
  })

  globalThis.fetch = async (_input, init) => {
    const url = String(_input)
    if (url.endsWith("/jobs")) {
      requestBody = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>
      return jsonResponse({
        id: "job-1",
        name: "river.stage",
        enabled: true,
        file_path: "/tmp/data/river.stage.csv",
        schedule_minutes: 15,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [
          {
            csv_column: "value",
            datastream_id: "stream-1",
            datastream_name: "Stage Datastream",
          },
        ],
        recent_logs: [],
        status: "pending",
        status_message: "Ready for the first run",
        last_pushed_timestamp: null,
        last_run_at: null,
        last_error: null,
      })
    }

    if (url.endsWith("/config")) {
      return jsonResponse({
        version: 1,
        server: {
          auth_type: "apikey",
          url: "https://example.com",
          api_key: "secret",
          username: "",
          password: "",
          workspace_id: "workspace-123",
          workspace_name: "Primary Workspace",
        },
        jobs: [
          {
            id: "job-1",
            name: "river.stage",
            enabled: true,
            file_path: "/tmp/data/river.stage.csv",
            schedule_minutes: 15,
            file_config: {
              headerRow: 1,
              dataStartRow: 2,
              delimiter: ",",
              identifierType: "name",
              timestamp: {
                key: "recorded_at",
                format: "ISO8601",
                timezoneMode: "embeddedOffset",
              },
            },
            column_mappings: [
              {
                csv_column: "value",
                datastream_id: "stream-1",
                datastream_name: "Stage Datastream",
              },
            ],
          },
        ],
      })
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  state.connectionSummary = {
    ok: true,
    state: "connected",
    message: "Connected",
    invalid_field: null,
    instance_name: "HydroServer",
    workspace_id: "workspace-123",
    workspace_name: "Primary Workspace",
    workspace_count: 1,
    datastream_count: 2,
    permissions_ok: true,
  }
  state.lastConnectionState = "connected"
  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [],
  }
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/data/river.stage.csv"
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
  state.validatedColumnMappings = [
    {
      csv_column: "value",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
  ]

  await createPipelineDatasource()

  assert.deepEqual(requestBody, {
    name: "river.stage",
    enabled: true,
    file_path: "/tmp/data/river.stage.csv",
    file_config: {
      headerRow: 1,
      dataStartRow: 2,
      delimiter: ",",
      identifierType: "name",
      timestamp: {
        key: "recorded_at",
        format: "ISO8601",
        timezoneMode: "embeddedOffset",
      },
    },
    column_mappings: [
      {
        csv_column: "value",
        datastream_id: "stream-1",
        datastream_name: "Stage Datastream",
      },
    ],
  })
  assert.equal(state.pipelineForm.filePath, "")
  assert.equal(state.pipelinePreview, null)
  assert.equal(state.pipelineReadyForMapping, false)
  assert.deepEqual(state.validatedColumnMappings, [])
  assert.deepEqual(state.pipelineMappingDrafts, [])
  assert.equal(state.pipelineCreateSubmitting, false)
  assert.equal(globalThis.window.location.hash, "#dashboard")
  assert.equal(state.config?.jobs.length, 1)
})

test("editPipelineSourceFile preloads an existing datasource on step 1", async () => {
  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#dashboard" } },
    configurable: true,
    writable: true,
  })

  globalThis.fetch = async (_input) => {
    const url = String(_input)
    if (url.includes("/csv/preview")) {
      return jsonResponse(createPreview())
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [
      {
        id: "job-1",
        name: "existing-source",
        enabled: true,
        file_path: "/tmp/data/existing.csv",
        schedule_minutes: 15,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [
          {
            csv_column: "value",
            datastream_id: "stream-1",
            datastream_name: "Stage Datastream",
          },
        ],
      },
    ],
  }

  await editPipelineSourceFile("job-1")

  assert.equal(state.pipelineEditTarget?.jobId, "job-1")
  assert.equal(state.pipelineEditorStartStep, 1)
  assert.equal(state.pipelineForm.filePath, "/tmp/data/existing.csv")
  assert.notEqual(state.pipelinePreview, null)
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
  assert.equal(globalThis.window.location.hash, "#jobs/new")
})

test("editPipelineCsvSetup opens the existing datasource on step 2", async () => {
  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#dashboard" } },
    configurable: true,
    writable: true,
  })

  globalThis.fetch = async (_input) => {
    const url = String(_input)
    if (url.includes("/csv/preview")) {
      return jsonResponse(createPreview())
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [
      {
        id: "job-1",
        name: "existing-source",
        enabled: true,
        file_path: "/tmp/data/existing.csv",
        schedule_minutes: 15,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [],
      },
    ],
  }

  await editPipelineCsvSetup("job-1")

  assert.equal(state.pipelineEditTarget?.jobId, "job-1")
  assert.equal(state.pipelineEditorStartStep, 2)
  assert.notEqual(state.pipelinePreview, null)
  assert.equal(globalThis.window.location.hash, "#jobs/new")
})

test("editPipelineMappings preloads mappings and routes to step 3", async () => {
  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#dashboard" } },
    configurable: true,
    writable: true,
  })

  globalThis.fetch = async (_input) => {
    const url = String(_input)
    if (url.includes("/csv/preview")) {
      return jsonResponse(createPreview())
    }

    if (url.endsWith("/datastreams")) {
      return jsonResponse([
        {
          id: "stream-1",
          name: "Stage Datastream",
          thing_id: "thing-1",
          thing_name: "Station 1",
          observed_property_name: "Stage",
          processing_level_definition: "Raw",
          unit_name: "meter",
          unit_symbol: "m",
          sampled_medium: "Water",
          sensor_name: "Sensor 1",
          result_type: "number",
        },
      ])
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [
      {
        id: "job-1",
        name: "existing-source",
        enabled: true,
        file_path: "/tmp/data/existing.csv",
        schedule_minutes: 15,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [
          {
            csv_column: "value",
            datastream_id: "stream-1",
            datastream_name: "Stage Datastream",
          },
        ],
      },
    ],
  }

  await editPipelineMappings("job-1")

  assert.equal(state.pipelineEditTarget?.jobId, "job-1")
  assert.equal(state.pipelineReadyForMapping, true)
  assert.deepEqual(state.pipelineMappingDrafts, [
    {
      csvColumn: "value",
      thingId: "thing-1",
      datastreamId: "stream-1",
    },
  ])
  assert.deepEqual(state.validatedColumnMappings, [
    {
      csv_column: "value",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
  ])
  assert.equal(globalThis.window.location.hash, "#jobs/new/mapping")
})

test("createPipelineDatasource updates an existing datasource when editing", async () => {
  let requestUrl = ""
  let requestMethod = ""
  let requestBody: Record<string, unknown> | null = null

  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#jobs/new/mapping" } },
    configurable: true,
    writable: true,
  })

  globalThis.fetch = async (_input, init) => {
    const url = String(_input)
    if (url.endsWith("/jobs/job-1")) {
      requestUrl = url
      requestMethod = String(init?.method ?? "")
      requestBody = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>
      return jsonResponse({
        id: "job-1",
        name: "existing-source",
        enabled: false,
        file_path: "/tmp/data/updated.csv",
        schedule_minutes: 30,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [
          {
            csv_column: "value",
            datastream_id: "stream-1",
            datastream_name: "Stage Datastream",
          },
        ],
        recent_logs: [],
        status: "pending",
        status_message: "Ready for the next run",
        last_pushed_timestamp: null,
        last_run_at: null,
        last_error: null,
      })
    }

    if (url.endsWith("/config")) {
      return jsonResponse({
        version: 1,
        server: {
          auth_type: "apikey",
          url: "https://example.com",
          api_key: "secret",
          username: "",
          password: "",
          workspace_id: "workspace-123",
          workspace_name: "Primary Workspace",
        },
        jobs: [
          {
            id: "job-1",
            name: "existing-source",
            enabled: false,
            file_path: "/tmp/data/updated.csv",
            schedule_minutes: 30,
            file_config: {
              headerRow: 1,
              dataStartRow: 2,
              delimiter: ",",
              identifierType: "name",
              timestamp: {
                key: "recorded_at",
                format: "ISO8601",
                timezoneMode: "embeddedOffset",
              },
            },
            column_mappings: [
              {
                csv_column: "value",
                datastream_id: "stream-1",
                datastream_name: "Stage Datastream",
              },
            ],
          },
        ],
      })
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  state.connectionSummary = {
    ok: true,
    state: "connected",
    message: "Connected",
    invalid_field: null,
    instance_name: "HydroServer",
    workspace_id: "workspace-123",
    workspace_name: "Primary Workspace",
    workspace_count: 1,
    datastream_count: 2,
    permissions_ok: true,
  }
  state.lastConnectionState = "connected"
  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [
      {
        id: "job-1",
        name: "existing-source",
        enabled: false,
        file_path: "/tmp/data/existing.csv",
        schedule_minutes: 30,
        file_config: {
          headerRow: 1,
          dataStartRow: 2,
          delimiter: ",",
          identifierType: "name",
          timestamp: {
            key: "recorded_at",
            format: "ISO8601",
            timezoneMode: "embeddedOffset",
          },
        },
        column_mappings: [
          {
            csv_column: "value",
            datastream_id: "stream-1",
            datastream_name: "Stage Datastream",
          },
        ],
      },
    ],
  }
  state.pipelineEditTarget = {
    jobId: "job-1",
    name: "existing-source",
    enabled: false,
    scheduleMinutes: 30,
  }
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/data/updated.csv"
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
  state.validatedColumnMappings = [
    {
      csv_column: "value",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
  ]

  await createPipelineDatasource()

  assert.equal(requestMethod, "PUT")
  assert.match(requestUrl, /\/jobs\/job-1$/)
  assert.deepEqual(requestBody, {
    name: "existing-source",
    enabled: false,
    file_path: "/tmp/data/updated.csv",
    schedule_minutes: 30,
    file_config: {
      headerRow: 1,
      dataStartRow: 2,
      delimiter: ",",
      identifierType: "name",
      timestamp: {
        key: "recorded_at",
        format: "ISO8601",
        timezoneMode: "embeddedOffset",
      },
    },
    column_mappings: [
      {
        csv_column: "value",
        datastream_id: "stream-1",
        datastream_name: "Stage Datastream",
      },
    ],
  })
  assert.equal(state.pipelineEditTarget, null)
  assert.equal(globalThis.window.location.hash, "#dashboard")
  assert.equal(state.config?.jobs[0]?.file_path, "/tmp/data/updated.csv")
})

test("createPipelineDatasource blocks submission when no columns are mapped", async () => {
  let fetchCalled = false

  globalThis.fetch = async () => {
    fetchCalled = true
    return jsonResponse({})
  }

  state.connectionSummary = {
    ok: true,
    state: "connected",
    message: "Connected",
    invalid_field: null,
    instance_name: "HydroServer",
    workspace_id: "workspace-123",
    workspace_name: "Primary Workspace",
    workspace_count: 1,
    datastream_count: 2,
    permissions_ok: true,
  }
  state.lastConnectionState = "connected"
  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [],
  }
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/data/river.stage.csv"
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

  await createPipelineDatasource()

  assert.equal(fetchCalled, false)
  assert.equal(state.pipelineCreateSubmitting, false)
})

test("createPipelineDatasource clears submitting state and keeps step-3 state on failure", async () => {
  globalThis.fetch = async () =>
    new Response(JSON.stringify({ detail: "Create failed" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    })

  state.connectionSummary = {
    ok: true,
    state: "connected",
    message: "Connected",
    invalid_field: null,
    instance_name: "HydroServer",
    workspace_id: "workspace-123",
    workspace_name: "Primary Workspace",
    workspace_count: 1,
    datastream_count: 2,
    permissions_ok: true,
  }
  state.lastConnectionState = "connected"
  state.config = {
    version: 1,
    server: {
      auth_type: "apikey",
      url: "https://example.com",
      api_key: "secret",
      username: "",
      password: "",
      workspace_id: "workspace-123",
      workspace_name: "Primary Workspace",
    },
    jobs: [],
  }
  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/data/river.stage.csv"
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
  state.validatedColumnMappings = [
    {
      csv_column: "value",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
  ]

  await createPipelineDatasource()

  assert.equal(state.pipelineForm.filePath, "/tmp/data/river.stage.csv")
  assert.notEqual(state.pipelinePreview, null)
  assert.equal(state.pipelineCreateSubmitting, false)
})

test("abandonPipelineCreation resets the wizard and returns to the dashboard", () => {
  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#jobs/new/mapping" } },
    configurable: true,
    writable: true,
  })

  state.pipelinePreview = createPreview()
  state.pipelineForm.filePath = "/tmp/data/river.stage.csv"
  state.pipelineSelectionTarget = "header-row"
  state.pipelineValidationAttempted = true
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
  state.pipelineMappingDrafts = [
    {
      csvColumn: "value",
      thingId: "thing-1",
      datastreamId: "stream-1",
    },
  ]
  state.validatedColumnMappings = [
    {
      csv_column: "value",
      datastream_id: "stream-1",
      datastream_name: "Stage Datastream",
    },
  ]
  state.pipelineCreateSubmitting = true

  abandonPipelineCreation()

  assert.equal(state.pipelineForm.filePath, "")
  assert.equal(state.pipelinePreview, null)
  assert.equal(state.pipelineSelectionTarget, null)
  assert.equal(state.pipelineValidationAttempted, false)
  assert.equal(state.pipelineReadyForMapping, false)
  assert.equal(state.validatedPipelineSettings, null)
  assert.deepEqual(state.pipelineMappingDrafts, [])
  assert.deepEqual(state.validatedColumnMappings, [])
  assert.equal(state.pipelineCreateSubmitting, false)
  assert.equal(globalThis.window.location.hash, "#dashboard")
})
