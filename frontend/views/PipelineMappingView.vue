<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type CSSProperties,
} from "vue"

import type { DatastreamSummary } from "../api"
import type {
  MappingDatastreamBrowserEntry,
  PipelineMappingRow,
} from "../composables/useMapping"
import { useAppModel } from "../composables/useAppModel"
import { navigate } from "../router"

const SECTION_HEADER_HEIGHT = 28
const DIVIDER_HEIGHT = 12
const THING_HEADER_HEIGHT = 26
const DATASTREAM_CARD_HEIGHT = 48
const VIRTUAL_OVERSCAN = 360

const MAPPING_PALETTE = [
  {
    surface: "#dbeafe",
    border: "#7fb5ff",
    text: "#184d8f",
    badge: "#3b82f6",
  },
  {
    surface: "#d7f3ea",
    border: "#6dd4b2",
    text: "#115c4b",
    badge: "#10b981",
  },
  {
    surface: "#f9ecd0",
    border: "#f2be5d",
    text: "#80541b",
    badge: "#f59e0b",
  },
  {
    surface: "#f3dce8",
    border: "#e58ab7",
    text: "#7a3f57",
    badge: "#ec4899",
  },
  {
    surface: "#d9e8fb",
    border: "#84b7f7",
    text: "#315f99",
    badge: "#6366f1",
  },
] as const

type ConnectorEntry =
  | {
      kind: "section"
      key: string
      label: string
    }
  | {
      kind: "divider"
      key: string
    }
  | {
      kind: "thing"
      key: string
      thingName: string
    }
  | {
      kind: "datastream"
      key: string
      datastream: DatastreamSummary
      mappedCsvColumn: string | null
      disabled: boolean
      current: boolean
    }

const emit = defineEmits<{
  "update:mappings": [Record<string, string>]
}>()

const model = useAppModel()

const validatedSettings = computed(() => model.state.validatedPipelineSettings)

const previewFileName = computed(
  () =>
    model.state.pipelineForm.filePath.split(/[\\/]/).filter(Boolean).at(-1) ??
    model.state.pipelineForm.filePath
)

const hasDatastreamInventory = computed(
  () => model.state.pipelineDatastreams.length > 0
)

const mappingRows = computed(() => model.pipelineMappingRows.value)
const browserEntries = computed(() => model.pipelineDatastreamBrowserEntries.value)
const activeCsvColumn = ref("")
const datastreamViewportRef = ref<HTMLElement | null>(null)
const datastreamScrollTop = ref(0)
const datastreamViewportHeight = ref(640)

let datastreamResizeObserver: ResizeObserver | null = null

const activeMappingRow = computed(
  () =>
    mappingRows.value.find((row) => row.csvColumn === activeCsvColumn.value) ?? null
)

const mappedColumnCount = computed(
  () => mappingRows.value.filter((row) => row.datastreamId).length
)

const mappingRecord = computed<Record<string, string>>(() =>
  Object.fromEntries(
    mappingRows.value.flatMap((row) =>
      row.datastreamId ? [[row.csvColumn, row.datastreamId]] : []
    )
  )
)

const sourceOrderByColumn = computed(
  () => new Map(mappingRows.value.map((row, index) => [row.csvColumn, index]))
)

const leftPanelHint = computed(() => {
  if (!activeMappingRow.value) {
    return mappedColumnCount.value > 0
      ? "Select a column to review or change its mapping."
      : "Select a column to begin mapping."
  }
  const col = activeMappingRow.value
  if (col.datastreamId && col.selectedDatastream) {
    const dsName = datastreamTitle(col.selectedDatastream)
    return `<b>${col.label}</b> mapped to <b>${dsName}</b> — click to remap, or click the mapping again to remove.`
  }
  return `<b>${col.label}</b> selected — now click a datastream on the right.`
})

const rightPanelHint = computed(() => {
  if (!activeMappingRow.value) {
    return mappedColumnCount.value > 0
      ? "Mapped datastreams shown with color badges."
      : "Choose a column first."
  }
  const col = activeMappingRow.value
  if (col.datastreamId && col.selectedDatastream) {
    const dsName = datastreamTitle(col.selectedDatastream)
    return `Click <b>${dsName}</b> again to unmap, or choose a different datastream.`
  }
  return `Showing all datastreams. Click one to connect it to <b>${col.label}</b>.`
})

const connectorEntries = computed<ConnectorEntry[]>(() =>
  buildConnectorEntries(browserEntries.value, activeMappingRow.value)
)

const virtualEntries = computed(() => {
  let top = 0

  return connectorEntries.value.map((entry) => {
    const height =
      entry.kind === "section"
        ? SECTION_HEADER_HEIGHT
        : entry.kind === "divider"
        ? DIVIDER_HEIGHT
        : entry.kind === "thing"
        ? THING_HEADER_HEIGHT
        : DATASTREAM_CARD_HEIGHT

    const next = {
      ...entry,
      top,
      height,
    }

    top += height
    return next
  })
})

const virtualHeight = computed(() => {
  const lastEntry = virtualEntries.value.at(-1)
  return lastEntry ? lastEntry.top + lastEntry.height : 0
})

const visibleEntries = computed(() => {
  const start = Math.max(datastreamScrollTop.value - VIRTUAL_OVERSCAN, 0)
  const end =
    datastreamScrollTop.value + datastreamViewportHeight.value + VIRTUAL_OVERSCAN

  return virtualEntries.value.filter(
    (entry) => entry.top + entry.height >= start && entry.top <= end
  )
})

watch(
  validatedSettings,
  () => {
    model.syncPipelineMappingDrafts()
  },
  { immediate: true }
)

watch(
  mappingRows,
  (rows) => {
    if (rows.length === 0) {
      activeCsvColumn.value = ""
      return
    }

    if (
      activeCsvColumn.value &&
      rows.some((row) => row.csvColumn === activeCsvColumn.value)
    ) {
      return
    }

    activeCsvColumn.value = ""
  },
  { immediate: true }
)

watch(
  mappingRecord,
  (next) => {
    emit("update:mappings", next)
  },
  { immediate: true }
)

watch(
  [activeCsvColumn, () => activeMappingRow.value?.datastreamId ?? ""],
  () => {
    resetDatastreamScroll()
  }
)

onMounted(() => {
  model.syncPipelineMappingDrafts()
  void model.loadPipelineDatastreams()
  measureDatastreamViewport()

  if (typeof ResizeObserver !== "undefined" && datastreamViewportRef.value) {
    datastreamResizeObserver = new ResizeObserver(() => {
      measureDatastreamViewport()
    })
    datastreamResizeObserver.observe(datastreamViewportRef.value)
  }
})

onBeforeUnmount(() => {
  datastreamResizeObserver?.disconnect()
})

function buildConnectorEntries(
  entries: MappingDatastreamBrowserEntry[],
  activeRow: PipelineMappingRow | null
): ConnectorEntry[] {
  const list: ConnectorEntry[] = []
  const otherEntries: ConnectorEntry[] = []
  let pendingThingName = ""

  if (activeRow?.selectedDatastream) {
    list.push({
      kind: "section",
      key: "currently-mapped",
      label: "Currently mapped",
    })
    list.push({
      kind: "datastream",
      key: `current-${activeRow.selectedDatastream.id}`,
      datastream: activeRow.selectedDatastream,
      mappedCsvColumn: activeRow.csvColumn,
      disabled: false,
      current: true,
    })
  }

  for (const entry of entries) {
    if (entry.kind === "thing") {
      pendingThingName = entry.thingName
      continue
    }

    if (activeRow?.datastreamId && entry.datastream.id === activeRow.datastreamId) {
      continue
    }

    const disabled = !isDatastreamAvailable(entry.datastream.id, entry.mappedCsvColumn)

    if (pendingThingName) {
      otherEntries.push({
        kind: "thing",
        key: `thing-${entry.datastream.thing_id}-${entry.datastream.id}`,
        thingName: pendingThingName,
      })
      pendingThingName = ""
    }

    otherEntries.push({
      kind: "datastream",
      key: entry.key,
      datastream: entry.datastream,
      mappedCsvColumn: entry.mappedCsvColumn,
      disabled,
      current: false,
    })
  }

  if (list.length > 0 && otherEntries.length > 0) {
    list.push({ kind: "divider", key: "connector-divider" })
    list.push({
      kind: "section",
      key: "other-datastreams",
      label: "Other datastreams",
    })
  }

  return [...list, ...otherEntries]
}

function measureDatastreamViewport(): void {
  datastreamViewportHeight.value = datastreamViewportRef.value?.clientHeight ?? 640
}

function resetDatastreamScroll(): void {
  datastreamScrollTop.value = 0
  if (datastreamViewportRef.value) {
    datastreamViewportRef.value.scrollTop = 0
  }
}

function onDatastreamScroll(event: Event): void {
  const target = event.target as HTMLElement | null
  datastreamScrollTop.value = target?.scrollTop ?? 0
}

function selectMappingColumn(csvColumn: string): void {
  activeCsvColumn.value = activeCsvColumn.value === csvColumn ? "" : csvColumn
}

function connectDatastream(datastreamId: string): void {
  if (!activeMappingRow.value) return
  model.updatePipelineMappingDatastream(activeMappingRow.value.csvColumn, datastreamId)
}

function datastreamTitle(datastream: DatastreamSummary): string {
  return datastream.observed_property_name || datastream.name
}

function datastreamThing(datastream: DatastreamSummary): string {
  return datastream.thing_name
}

function columnTargetLabel(row: PipelineMappingRow): string {
  if (!row.selectedDatastream) return ""
  return datastreamTitle(row.selectedDatastream)
}

function mappingNumber(csvColumn: string | null): number | null {
  if (!csvColumn) return null
  const index = sourceOrderByColumn.value.get(csvColumn)
  return index === undefined ? null : index + 1
}

function mappingToneStyle(csvColumn: string | null): CSSProperties | undefined {
  const number = mappingNumber(csvColumn)
  if (number === null) return undefined

  const tone = MAPPING_PALETTE[(number - 1) % MAPPING_PALETTE.length]
  return {
    "--mapping-surface": tone.surface,
    "--mapping-border": tone.border,
    "--mapping-text": tone.text,
    "--mapping-badge": tone.badge,
    "--mapping-badge-text": "#ffffff",
  } as CSSProperties
}

function isDatastreamAvailable(
  datastreamId: string,
  mappedCsvColumn: string | null
): boolean {
  if (!activeMappingRow.value) return false

  if (activeMappingRow.value.datastreamId) {
    return activeMappingRow.value.datastreamId === datastreamId
  }

  return !mappedCsvColumn
}

function isColumnMapped(row: PipelineMappingRow): boolean {
  return Boolean(row.datastreamId)
}

function isColumnSelected(row: PipelineMappingRow): boolean {
  return row.csvColumn === activeCsvColumn.value
}

function isDatastreamMapped(entry: ConnectorEntry): boolean {
  return entry.kind === "datastream" && Boolean(entry.mappedCsvColumn)
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <h1 class="page-title">
            Data source creation step 3/3 - source to datastream mapping for
            {{ previewFileName }}
          </h1>
        </div>
        <div class="button-row wizard-actions">
          <button class="btn-ghost" type="button" @click="navigate('jobs-new')">
            <span aria-hidden="true">&larr;</span>
            <span>Back to CSV Setup</span>
          </button>
        </div>
      </div>
    </header>

    <article class="pipeline-subcard mapping-subcard">
      <div v-if="model.state.pipelineDatastreamsLoading" class="empty-panel">
        <div class="empty-icon">...</div>
        <p class="section-copy">Loading HydroServer datastreams.</p>
      </div>

      <div v-else-if="!hasDatastreamInventory" class="empty-panel">
        <div class="empty-icon">0</div>
        <p class="section-copy">
          No datastreams were returned for the connected workspace.
        </p>
      </div>

      <div v-else-if="mappingRows.length === 0" class="empty-panel">
        <div class="empty-icon">TS</div>
        <p class="section-copy">
          No value columns are available to map after excluding the timestamp
          column.
        </p>
      </div>

      <div v-else class="mapping-connector-shell">
        <section class="mapping-connector-panel">
          <header class="mapping-connector-header">
            <div class="mapping-connector-header-row">
              <p class="mapping-connector-title">CSV columns</p>
              <p class="mapping-connector-header-meta">
                <span class="mapping-connector-header-count">
                  {{ mappedColumnCount }}
                </span>
                <span>of {{ mappingRows.length }} mapped</span>
              </p>
            </div>
          </header>

          <div class="mapping-connector-body">
            <div class="mapping-column-scroll">
              <button
                v-for="row in mappingRows"
                :key="row.csvColumn"
                class="mapping-column-item"
                :class="{
                  'mapping-column-item-selected': isColumnSelected(row),
                  'mapping-connector-item-mapped': isColumnMapped(row),
                }"
                :style="isColumnMapped(row) ? mappingToneStyle(row.csvColumn) : undefined"
                type="button"
                @click="selectMappingColumn(row.csvColumn)"
              >
                <span
                  class="mapping-item-badge"
                  :class="{ 'mapping-item-badge-filled': isColumnMapped(row) }"
                >
                  {{ isColumnMapped(row) ? mappingNumber(row.csvColumn) : "" }}
                </span>
                <span class="mapping-column-item-copy">
                  <span class="mapping-column-item-name">{{ row.label }}</span>
                  <span v-if="row.selectedDatastream" class="mapping-column-item-target">
                    <span aria-hidden="true">-&gt;</span>
                    <span>{{ columnTargetLabel(row) }}</span>
                  </span>
                </span>
              </button>
            </div>
          </div>

          <footer class="mapping-connector-footer" v-html="leftPanelHint" />
        </section>

        <section class="mapping-connector-panel">
          <header class="mapping-connector-header">
            <p class="mapping-connector-title">Datastreams</p>
          </header>

          <div
            ref="datastreamViewportRef"
            class="mapping-connector-body mapping-datastream-scroll"
            @scroll="onDatastreamScroll"
          >
            <div
              class="mapping-virtual-stage"
              :style="{ height: `${virtualHeight}px` }"
            >
              <div
                v-for="entry in visibleEntries"
                :key="entry.key"
                class="mapping-virtual-item"
                :style="{
                  transform: `translateY(${entry.top}px)`,
                  height: `${entry.height}px`,
                }"
              >
                <div v-if="entry.kind === 'section'" class="mapping-connector-section">
                  {{ entry.label }}
                </div>

                <div v-else-if="entry.kind === 'divider'" class="mapping-connector-divider" />

                <div v-else-if="entry.kind === 'thing'" class="mapping-thing-group">
                  {{ entry.thingName }}
                </div>

                <button
                  v-else
                  class="mapping-datastream-item"
                  :class="{
                    'mapping-connector-item-mapped': isDatastreamMapped(entry),
                    'mapping-datastream-item-current': entry.current,
                    'mapping-datastream-item-disabled': entry.disabled,
                  }"
                  :style="
                    isDatastreamMapped(entry)
                      ? mappingToneStyle(entry.mappedCsvColumn)
                      : undefined
                  "
                  :disabled="entry.disabled"
                  type="button"
                  @click="connectDatastream(entry.datastream.id)"
                >
                  <span
                    class="mapping-item-badge"
                    :class="{ 'mapping-item-badge-filled': isDatastreamMapped(entry) }"
                  >
                    {{ isDatastreamMapped(entry) ? mappingNumber(entry.mappedCsvColumn) : "" }}
                  </span>
                  <span class="mapping-datastream-item-copy">
                    <span class="mapping-datastream-item-name">
                      {{ datastreamTitle(entry.datastream) }}
                    </span>
                  </span>
                  <span class="mapping-datastream-item-thing">
                    {{ datastreamThing(entry.datastream) }}
                  </span>
                </button>
              </div>
            </div>
          </div>

          <footer class="mapping-connector-footer" v-html="rightPanelHint" />
        </section>
      </div>
    </article>
  </section>
</template>
