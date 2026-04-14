<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type CSSProperties,
} from 'vue'

import type { DatastreamSummary } from '../api'
import AccountMenuButton from '../components/AccountMenuButton.vue'
import AnimatedLoadingIcon from '../components/AnimatedLoadingIcon.vue'
import type {
  MappingDatastreamBrowserEntry,
  PipelineMappingRow,
} from '../composables/useMapping'
import { useAppModel } from '../composables/useAppModel'
import { navigate } from '../router'

const SECTION_HEADER_HEIGHT = 28
const DIVIDER_HEIGHT = 12
const THING_HEADER_HEIGHT = 34
const DATASTREAM_CARD_HEIGHT = 48
const VIRTUAL_OVERSCAN = 360

const MAPPING_PALETTE = [
  {
    surface: '#d9ebe7',
    border: '#6ea79d',
    text: '#214943',
    badge: '#2f7d6f',
  },
  {
    surface: '#dbe7ef',
    border: '#7699b6',
    text: '#24465e',
    badge: '#426e92',
  },
  {
    surface: '#eee2d3',
    border: '#b79262',
    text: '#62462a',
    badge: '#9a6d35',
  },
  {
    surface: '#eadde3',
    border: '#ab7e90',
    text: '#5e3949',
    badge: '#8e556c',
  },
  {
    surface: '#e4e1ef',
    border: '#8f88af',
    text: '#494264',
    badge: '#696185',
  },
] as const

type ConnectorEntry =
  | {
      kind: 'section'
      key: string
      label: string
    }
  | {
      kind: 'divider'
      key: string
    }
  | {
      kind: 'thing'
      key: string
      thingName: string
    }
  | {
      kind: 'datastream'
      key: string
      datastream: DatastreamSummary
      mappedCsvColumn: string | null
      disabled: boolean
      current: boolean
    }

const emit = defineEmits<{
  'update:mappings': [Record<string, string>]
}>()

const model = useAppModel()

const validatedSettings = computed(() => model.state.validatedPipelineSettings)

const previewFileName = computed(
  () =>
    model.state.pipelineForm.filePath.split(/[\\/]/).filter(Boolean).at(-1) ??
    model.state.pipelineForm.filePath
)
const isEditing = computed(() => Boolean(model.state.pipelineEditTarget))

const wizardStepLabel = computed(() =>
  isEditing.value ? 'Edit Data Source · Step 3 of 3' : 'Data Source Creation · Step 3 of 3'
)
const wizardTitle = computed(() =>
  isEditing.value ? 'Edit Source Mappings' : 'Map Columns to Datastreams'
)

const hasDatastreamInventory = computed(
  () => model.state.pipelineDatastreams.length > 0
)

const mappingRows = computed(() => model.pipelineMappingRows.value)
const browserEntries = computed(
  () => model.pipelineDatastreamBrowserEntries.value
)
const activeCsvColumn = ref('')
const columnNameFilter = ref('')
const datastreamThingFilter = ref('')
const datastreamNameFilter = ref('')
const datastreamViewportRef = ref<HTMLElement | null>(null)
const datastreamScrollTop = ref(0)
const datastreamViewportHeight = ref(640)

let datastreamResizeObserver: ResizeObserver | null = null

const activeMappingRow = computed(
  () =>
    mappingRows.value.find((row) => row.csvColumn === activeCsvColumn.value) ??
    null
)

const currentMappedDatastream = computed(
  () => activeMappingRow.value?.selectedDatastream ?? null
)

const mappedColumnCount = computed(
  () => mappingRows.value.filter((row) => row.datastreamId).length
)
const createButtonLabel = computed(() => {
  if (model.state.pipelineCreateSubmitting) {
    return isEditing.value ? 'Saving...' : 'Creating...'
  }

  return isEditing.value ? 'Save Changes' : 'Create'
})
const createDisabled = computed(
  () =>
    model.state.pipelineCreateSubmitting ||
    model.state.pipelineDatastreamsLoading ||
    mappingRows.value.length === 0 ||
    mappedColumnCount.value === 0
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

const columnFilterOptions = computed(() =>
  Array.from(new Set(mappingRows.value.map((row) => row.label))).sort((a, b) =>
    a.localeCompare(b)
  )
)

const thingFilterOptions = computed(() =>
  model.pipelineThingOptions.value.map((option) => option.name)
)

const datastreamNameFilterOptions = computed(() =>
  Array.from(
    new Set(
      model.state.pipelineDatastreams
        .map((datastream) => datastream.name)
        .filter(Boolean)
    )
  ).sort((a, b) => a.localeCompare(b))
)

const filteredMappingRows = computed(() => {
  const query = normalizeFilter(columnNameFilter.value)
  if (!query) return mappingRows.value

  return mappingRows.value.filter((row) =>
    row.label.toLowerCase().includes(query)
  )
})

const filteredBrowserEntries = computed(() =>
  filterDatastreamEntries(browserEntries.value, {
    thingQuery: datastreamThingFilter.value,
    datastreamQuery: datastreamNameFilter.value,
  })
)

const connectorEntries = computed<ConnectorEntry[]>(() =>
  buildConnectorEntries(filteredBrowserEntries.value, activeMappingRow.value)
)

const virtualEntries = computed(() => {
  let top = 0

  return connectorEntries.value.map((entry) => {
    const height =
      entry.kind === 'section'
        ? SECTION_HEADER_HEIGHT
        : entry.kind === 'divider'
        ? DIVIDER_HEIGHT
        : entry.kind === 'thing'
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
    datastreamScrollTop.value +
    datastreamViewportHeight.value +
    VIRTUAL_OVERSCAN

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
      activeCsvColumn.value = ''
      return
    }

    if (
      activeCsvColumn.value &&
      rows.some((row) => row.csvColumn === activeCsvColumn.value)
    ) {
      return
    }

    activeCsvColumn.value =
      rows.find((row) => row.datastreamId)?.csvColumn ?? ''
  },
  { immediate: true }
)

watch(
  mappingRecord,
  (next) => {
    emit('update:mappings', next)
  },
  { immediate: true }
)

watch([datastreamThingFilter, datastreamNameFilter], () => {
  resetDatastreamScroll()
})

onMounted(() => {
  model.syncPipelineMappingDrafts()
  void model.loadPipelineDatastreams()
  measureDatastreamViewport()

  if (typeof ResizeObserver !== 'undefined' && datastreamViewportRef.value) {
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
  let pendingThingName = ''

  for (const entry of entries) {
    if (entry.kind === 'thing') {
      pendingThingName = entry.thingName
      continue
    }

    if (
      activeRow?.datastreamId &&
      entry.datastream.id === activeRow.datastreamId
    ) {
      continue
    }

    const disabled = !isDatastreamAvailable(
      entry.datastream.id,
      entry.mappedCsvColumn
    )

    if (pendingThingName) {
      list.push({
        kind: 'thing',
        key: `thing-${entry.datastream.thing_id}-${entry.datastream.id}`,
        thingName: pendingThingName,
      })
      pendingThingName = ''
    }

    list.push({
      kind: 'datastream',
      key: entry.key,
      datastream: entry.datastream,
      mappedCsvColumn: entry.mappedCsvColumn,
      disabled,
      current: false,
    })
  }

  if (activeRow?.selectedDatastream && list.length > 0) {
    list.unshift({
      kind: 'section',
      key: 'other-datastreams',
      label: 'Other datastreams',
    })
  }

  return list
}

function filterDatastreamEntries(
  entries: MappingDatastreamBrowserEntry[],
  filters: { thingQuery: string; datastreamQuery: string }
): MappingDatastreamBrowserEntry[] {
  const thingQuery = normalizeFilter(filters.thingQuery)
  const datastreamQuery = normalizeFilter(filters.datastreamQuery)

  if (!thingQuery && !datastreamQuery) {
    return entries
  }

  const filtered: MappingDatastreamBrowserEntry[] = []
  let currentThing: Extract<
    MappingDatastreamBrowserEntry,
    { kind: 'thing' }
  > | null = null
  let emittedThing = false

  for (const entry of entries) {
    if (entry.kind === 'thing') {
      currentThing = entry
      emittedThing = false
      continue
    }

    const matchesThing =
      !thingQuery ||
      entry.datastream.thing_name.toLowerCase().includes(thingQuery) ||
      currentThing?.thingName.toLowerCase().includes(thingQuery) === true

    const matchesDatastream =
      !datastreamQuery ||
      entry.datastream.name.toLowerCase().includes(datastreamQuery)

    if (!matchesThing || !matchesDatastream) {
      continue
    }

    if (currentThing && !emittedThing) {
      filtered.push(currentThing)
      emittedThing = true
    }

    filtered.push(entry)
  }

  return filtered
}

function normalizeFilter(value: string): string {
  return value.trim().toLowerCase()
}

function measureDatastreamViewport(): void {
  datastreamViewportHeight.value =
    datastreamViewportRef.value?.clientHeight ?? 640
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
  activeCsvColumn.value = activeCsvColumn.value === csvColumn ? '' : csvColumn
}

function connectDatastream(datastreamId: string): void {
  if (!activeMappingRow.value) return
  const csvColumn = activeMappingRow.value.csvColumn
  model.updatePipelineMappingDatastream(csvColumn, datastreamId)
  activeCsvColumn.value = ''
}

function datastreamTitle(datastream: DatastreamSummary): string {
  const observedPropertyName = datastream.observed_property_name.trim()
  const unitSymbol = datastream.unit_symbol.trim()

  if (observedPropertyName && unitSymbol) {
    return `${observedPropertyName} (${unitSymbol})`
  }

  return observedPropertyName || datastream.name
}

function datastreamThing(datastream: DatastreamSummary): string {
  return datastream.name
}

function isMapped(csvColumn: string | null): boolean {
  return mappingNumber(csvColumn) !== null
}

function columnTargetLabel(row: PipelineMappingRow): string {
  if (!row.selectedDatastream) return ''
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
    '--mapping-surface': tone.surface,
    '--mapping-border': tone.border,
    '--mapping-text': tone.text,
    '--mapping-badge': tone.badge,
    '--mapping-badge-text': '#ffffff',
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
  return entry.kind === 'datastream' && Boolean(entry.mappedCsvColumn)
}
</script>

<template>
  <section
    class="page-shell animate-fade-in onboarding-shell pipeline-editor-shell pipeline-editor-shell-fullscreen"
  >
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <p class="wizard-step-label">{{ wizardStepLabel }}</p>
          <h1 class="wizard-page-title">{{ wizardTitle }}</h1>
        </div>
        <div class="button-row wizard-actions">
          <button
            v-if="isEditing"
            class="btn-ghost"
            type="button"
            @click="model.abandonPipelineCreation()"
          >
            Cancel
          </button>
          <button class="btn-ghost" type="button" @click="navigate('jobs-new')">
            <span aria-hidden="true">&larr;</span>
            <span>Back to CSV Setup</span>
          </button>
          <button
            class="btn-primary"
            type="button"
            :disabled="createDisabled"
            @click="model.createPipelineDatasource()"
          >
            {{ createButtonLabel }}
          </button>
          <AccountMenuButton />
        </div>
      </div>
    </header>

    <article class="pipeline-subcard mapping-subcard pipeline-mapping-workspace">
      <div v-if="model.state.pipelineDatastreamsLoading" class="empty-panel">
        <AnimatedLoadingIcon :size="96" />
        <p class="section-copy">Loading datastreams.</p>
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
            <div class="mapping-filter-grid mapping-filter-grid-single">
              <label class="mapping-filter-field">
                <span class="mapping-filter-label">Filter columns</span>
                <input
                  v-model="columnNameFilter"
                  class="input mapping-filter-input"
                  list="mapping-column-filter-options"
                  type="text"
                  placeholder="Type or select a column"
                  autocomplete="off"
                />
              </label>
              <datalist id="mapping-column-filter-options">
                <option
                  v-for="label in columnFilterOptions"
                  :key="label"
                  :value="label"
                />
              </datalist>
            </div>
          </header>

          <div class="mapping-connector-body">
            <div class="mapping-column-scroll">
              <div
                v-if="filteredMappingRows.length === 0"
                class="mapping-filter-empty"
              >
                No columns match the current filter.
              </div>
              <button
                v-for="row in filteredMappingRows"
                :key="row.csvColumn"
                class="mapping-column-item"
                :class="{
                  'mapping-column-item-selected': isColumnSelected(row),
                  'mapping-connector-item-mapped': isColumnMapped(row),
                }"
                :style="
                  isColumnMapped(row)
                    ? mappingToneStyle(row.csvColumn)
                    : undefined
                "
                type="button"
                @click="selectMappingColumn(row.csvColumn)"
              >
                <span
                  v-if="isColumnMapped(row)"
                  class="mapping-item-badge"
                  :class="{ 'mapping-item-badge-filled': isColumnMapped(row) }"
                >
                  {{ mappingNumber(row.csvColumn) }}
                </span>
                <span v-else class="mapping-item-dot" aria-hidden="true" />
                <span class="mapping-column-item-copy">
                  <span class="mapping-column-item-name">{{ row.label }}</span>
                  <span
                    v-if="row.selectedDatastream"
                    class="mapping-column-item-target"
                  >
                    <span aria-hidden="true">→</span>
                    <span>{{ columnTargetLabel(row) }}</span>
                  </span>
                </span>
              </button>
            </div>
          </div>
        </section>

        <section class="mapping-connector-panel">
          <header class="mapping-connector-header">
            <p class="mapping-connector-title">Datastreams</p>
            <div class="mapping-filter-grid">
              <label class="mapping-filter-field">
                <span class="mapping-filter-label">Site filter</span>
                <input
                  v-model="datastreamThingFilter"
                  class="input mapping-filter-input"
                  list="mapping-thing-filter-options"
                  type="text"
                  placeholder="Type or select a site"
                  autocomplete="off"
                />
              </label>
              <label class="mapping-filter-field">
                <span class="mapping-filter-label">Datastream filter</span>
                <input
                  v-model="datastreamNameFilter"
                  class="input mapping-filter-input"
                  list="mapping-datastream-filter-options"
                  type="text"
                  placeholder="Type or select a datastream"
                  autocomplete="off"
                />
              </label>
              <datalist id="mapping-thing-filter-options">
                <option
                  v-for="thing in thingFilterOptions"
                  :key="thing"
                  :value="thing"
                />
              </datalist>
              <datalist id="mapping-datastream-filter-options">
                <option
                  v-for="name in datastreamNameFilterOptions"
                  :key="name"
                  :value="name"
                />
              </datalist>
            </div>
          </header>

          <div
            ref="datastreamViewportRef"
            class="mapping-connector-body mapping-datastream-scroll"
            @scroll="onDatastreamScroll"
          >
            <div
              v-if="currentMappedDatastream"
              class="mapping-datastream-sticky"
            >
              <div class="mapping-connector-section">Currently mapped</div>
              <div
                v-if="currentMappedDatastream.thing_name"
                class="mapping-thing-group"
              >
                {{ currentMappedDatastream.thing_name }}
              </div>
              <button
                class="mapping-datastream-item mapping-connector-item-mapped mapping-datastream-item-current"
                :style="mappingToneStyle(activeMappingRow?.csvColumn ?? null)"
                type="button"
                @click="connectDatastream(currentMappedDatastream.id)"
              >
                <span class="mapping-item-badge mapping-item-badge-filled">
                  {{ mappingNumber(activeMappingRow?.csvColumn ?? null) }}
                </span>
                <span class="mapping-datastream-item-copy">
                  <span class="mapping-datastream-item-name">
                    {{ datastreamTitle(currentMappedDatastream) }}
                  </span>
                </span>
                <span class="mapping-datastream-item-thing">
                  {{ datastreamThing(currentMappedDatastream) }}
                </span>
              </button>
            </div>

            <div
              v-if="connectorEntries.length === 0"
              class="mapping-filter-empty"
            >
              {{
                currentMappedDatastream
                  ? 'No other datastreams match the current filters.'
                  : 'No datastreams match the current filters.'
              }}
            </div>
            <div
              v-else
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
                <div
                  v-if="entry.kind === 'section'"
                  class="mapping-connector-section"
                >
                  {{ entry.label }}
                </div>

                <div
                  v-else-if="entry.kind === 'divider'"
                  class="mapping-connector-divider"
                />

                <div
                  v-else-if="entry.kind === 'thing'"
                  class="mapping-thing-group"
                >
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
                    v-if="isMapped(entry.mappedCsvColumn)"
                    class="mapping-item-badge"
                    :class="{
                      'mapping-item-badge-filled': isDatastreamMapped(entry),
                    }"
                  >
                    {{ mappingNumber(entry.mappedCsvColumn) }}
                  </span>
                  <span v-else class="mapping-item-dot" aria-hidden="true" />
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
        </section>
      </div>
    </article>
  </section>
</template>
