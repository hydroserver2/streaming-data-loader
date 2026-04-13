import { computed } from "vue"

import {
  getDatastreams,
  type ColumnMapping,
  type DatastreamSummary,
} from "../api"
import { state, type PipelineMappingDraft } from "./state"
import { previewHeaders } from "./usePipeline"

function clearPipelineCreateFeedback(): void {
  state.pipelineCreateFeedback = null
}

export type MappingSourceColumn = {
  csvColumn: string
  label: string
}

export type MappingThingOption = {
  id: string
  name: string
}

export type PipelineMappingRow = MappingSourceColumn & {
  thingId: string
  datastreamId: string
  selectedDatastream: DatastreamSummary | null
}

export type MappingDatastreamBrowserEntry =
  | {
      kind: "thing"
      key: string
      thingId: string
      thingName: string
    }
  | {
      kind: "datastream"
      key: string
      datastream: DatastreamSummary
      mappedCsvColumn: string | null
      mappedColumnLabel: string | null
    }

export function buildMappingSourceColumns(
  headers: string[],
  identifierType: "name" | "index",
  timestampKey: string
): MappingSourceColumn[] {
  return headers
    .map((header, index) => {
      const csvColumn =
        identifierType === "index" ? String(index + 1) : header

      if (csvColumn === timestampKey) {
        return null
      }

      return {
        csvColumn,
        label:
          identifierType === "index" ? `${index + 1} · ${header}` : header,
      }
    })
    .filter((column): column is MappingSourceColumn => column !== null)
}

export const pipelineMappingSourceColumns = computed(() => {
  const settings = state.validatedPipelineSettings
  if (!settings) return []

  return buildMappingSourceColumns(
    previewHeaders.value,
    settings.identifierType,
    settings.timestamp.key
  )
})

export const pipelineThingOptions = computed<MappingThingOption[]>(() => {
  const things = new Map<string, string>()

  for (const datastream of state.pipelineDatastreams) {
    if (!datastream.thing_id || !datastream.thing_name) continue
    things.set(datastream.thing_id, datastream.thing_name)
  }

  return Array.from(things.entries())
    .map(([id, name]) => ({ id, name }))
    .sort((a, b) => a.name.localeCompare(b.name))
})

export const pipelineMappingRows = computed<PipelineMappingRow[]>(() =>
  pipelineMappingSourceColumns.value.map((source) => {
    const draft = mappingDraftByColumn(source.csvColumn)
    const selectedDatastream = draft?.datastreamId
      ? datastreamById(draft.datastreamId)
      : null

    return {
      ...source,
      thingId: selectedDatastream?.thing_id ?? draft?.thingId ?? "",
      datastreamId: selectedDatastream?.id ?? draft?.datastreamId ?? "",
      selectedDatastream,
    }
  })
)

export const pipelineDatastreamBrowserEntries =
  computed<MappingDatastreamBrowserEntry[]>(() =>
    buildDatastreamBrowserEntries(
      state.pipelineDatastreams,
      state.pipelineMappingDrafts,
      pipelineMappingSourceColumns.value
    )
  )

export async function loadPipelineDatastreams(force = false): Promise<void> {
  clearPipelineCreateFeedback()
  if (state.pipelineDatastreamsLoading) return

  if (state.pipelineDatastreams.length > 0 && !force) {
    syncPipelineMappingDrafts()
    return
  }

  state.pipelineDatastreamsLoading = true

  try {
    state.pipelineDatastreams = sortDatastreams(await getDatastreams())
    syncPipelineMappingDrafts()
  } catch {
    state.pipelineDatastreams = []
  } finally {
    state.pipelineDatastreamsLoading = false
  }
}

export function syncPipelineMappingDrafts(): void {
  const nextDrafts: PipelineMappingDraft[] = pipelineMappingSourceColumns.value.map(
    (source) => {
      const existing = mappingDraftByColumn(source.csvColumn)
      const selectedDatastream = existing?.datastreamId
        ? datastreamById(existing.datastreamId)
        : null

      return {
        csvColumn: source.csvColumn,
        thingId: selectedDatastream?.thing_id ?? existing?.thingId ?? "",
        datastreamId: selectedDatastream?.id ?? "",
      }
    }
  )

  state.pipelineMappingDrafts = nextDrafts
  syncValidatedColumnMappings()
}

export function datastreamOptionsForThing(
  thingId: string,
  csvColumn: string
): DatastreamSummary[] {
  const currentDatastreamId = mappingDraftByColumn(csvColumn)?.datastreamId ?? ""

  return sortDatastreams(
    state.pipelineDatastreams.filter(
      (datastream) =>
        datastream.thing_id === thingId &&
        (!isDatastreamMappedElsewhere(datastream.id, csvColumn) ||
          datastream.id === currentDatastreamId)
    )
  )
}

export function updatePipelineMappingThing(
  csvColumn: string,
  thingId: string
): void {
  clearPipelineCreateFeedback()
  syncPipelineMappingDrafts()

  const draft = mappingDraftByColumn(csvColumn)
  if (!draft) return

  draft.thingId = thingId

  const selectedDatastream = datastreamById(draft.datastreamId)
  if (!selectedDatastream || selectedDatastream.thing_id !== thingId) {
    draft.datastreamId = ""
  }

  syncValidatedColumnMappings()
}

export function updatePipelineMappingDatastream(
  csvColumn: string,
  datastreamId: string
): void {
  clearPipelineCreateFeedback()
  syncPipelineMappingDrafts()

  const draft = mappingDraftByColumn(csvColumn)
  if (!draft) return

  if (!datastreamId) {
    draft.datastreamId = ""
    syncValidatedColumnMappings()
    return
  }

  const datastream = datastreamById(datastreamId)
  if (!datastream) return

  const owner = state.pipelineMappingDrafts.find(
    (candidate) =>
      candidate.csvColumn !== csvColumn && candidate.datastreamId === datastream.id
  )

  if (draft.datastreamId === datastream.id) {
    draft.thingId = ""
    draft.datastreamId = ""
    syncValidatedColumnMappings()
    return
  }

  if (owner) {
    return
  }

  draft.thingId = datastream.thing_id
  draft.datastreamId = datastream.id

  syncValidatedColumnMappings()
}

export function clearPipelineMapping(csvColumn: string): void {
  clearPipelineCreateFeedback()
  syncPipelineMappingDrafts()

  const draft = mappingDraftByColumn(csvColumn)
  if (!draft) return

  draft.thingId = ""
  draft.datastreamId = ""

  syncValidatedColumnMappings()
}

export function buildPipelineColumnMappings(): ColumnMapping[] {
  return state.pipelineMappingDrafts.flatMap((draft) => {
    const datastream = datastreamById(draft.datastreamId)
    if (!datastream) return []

    return [
      {
        csv_column: draft.csvColumn,
        datastream_id: datastream.id,
        datastream_name: datastream.name,
      },
    ]
  })
}

function syncValidatedColumnMappings(): void {
  state.validatedColumnMappings = buildPipelineColumnMappings()
}

function datastreamById(datastreamId: string): DatastreamSummary | null {
  return (
    state.pipelineDatastreams.find(
      (datastream) => datastream.id === datastreamId
    ) ?? null
  )
}

function mappingDraftByColumn(csvColumn: string): PipelineMappingDraft | null {
  return (
    state.pipelineMappingDrafts.find((draft) => draft.csvColumn === csvColumn) ??
    null
  )
}

function isDatastreamMappedElsewhere(
  datastreamId: string,
  csvColumn: string
): boolean {
  return state.pipelineMappingDrafts.some(
    (draft) =>
      draft.csvColumn !== csvColumn && draft.datastreamId === datastreamId
  )
}

function sortDatastreams(
  datastreams: DatastreamSummary[]
): DatastreamSummary[] {
  return [...datastreams].sort((a, b) => {
    const thingCompare = a.thing_name.localeCompare(b.thing_name)
    if (thingCompare !== 0) return thingCompare

    const observedPropertyCompare = a.observed_property_name.localeCompare(
      b.observed_property_name
    )
    if (observedPropertyCompare !== 0) return observedPropertyCompare

    return a.name.localeCompare(b.name)
  })
}

export function buildDatastreamBrowserEntries(
  datastreams: DatastreamSummary[],
  drafts: PipelineMappingDraft[],
  sourceColumns: MappingSourceColumn[]
): MappingDatastreamBrowserEntry[] {
  const sourceLabelByColumn = new Map(
    sourceColumns.map((source) => [source.csvColumn, source.label])
  )
  const mappedColumnByDatastream = new Map(
    drafts
      .filter((draft) => draft.datastreamId)
      .map((draft) => [draft.datastreamId, draft.csvColumn])
  )
  const entries: MappingDatastreamBrowserEntry[] = []
  let currentThingId = ""

  for (const datastream of sortDatastreams(datastreams)) {
    if (datastream.thing_id !== currentThingId) {
      currentThingId = datastream.thing_id
      entries.push({
        kind: "thing",
        key: `thing-${datastream.thing_id}`,
        thingId: datastream.thing_id,
        thingName: datastream.thing_name,
      })
    }

    const mappedCsvColumn = mappedColumnByDatastream.get(datastream.id) ?? null

    entries.push({
      kind: "datastream",
      key: `datastream-${datastream.id}`,
      datastream,
      mappedCsvColumn,
      mappedColumnLabel: mappedCsvColumn
        ? sourceLabelByColumn.get(mappedCsvColumn) ?? mappedCsvColumn
        : null,
    })
  }

  return entries
}
