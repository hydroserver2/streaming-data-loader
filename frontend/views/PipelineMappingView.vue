<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"

import type { DatastreamSummary } from "../api"
import { useAppModel } from "../composables/useAppModel"
import { navigate } from "../router"

const THING_HEADER_HEIGHT = 34
const DATASTREAM_CARD_HEIGHT = 64
const VIRTUAL_OVERSCAN = 320

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

const datastreamVirtualEntries = computed(() => {
  let top = 0

  return browserEntries.value.map((entry) => {
    const height =
      entry.kind === "thing" ? THING_HEADER_HEIGHT : DATASTREAM_CARD_HEIGHT
    const next = {
      ...entry,
      height,
      top,
    }
    top += height
    return next
  })
})

const datastreamVirtualHeight = computed(() => {
  const lastEntry = datastreamVirtualEntries.value.at(-1)
  return lastEntry ? lastEntry.top + lastEntry.height : 0
})

const visibleDatastreamEntries = computed(() => {
  const start = Math.max(datastreamScrollTop.value - VIRTUAL_OVERSCAN, 0)
  const end =
    datastreamScrollTop.value + datastreamViewportHeight.value + VIRTUAL_OVERSCAN

  return datastreamVirtualEntries.value.filter(
    (entry) => entry.top + entry.height >= start && entry.top <= end
  )
})

const activeMappingPrompt = computed(() => {
  if (!activeMappingRow.value) return "Select a CSV column to start mapping."

  if (activeMappingRow.value.selectedDatastream) {
    return `Click another datastream to remap ${activeMappingRow.value.label}.`
  }

  return `Click a datastream to map ${activeMappingRow.value.label}.`
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

    const hasActiveRow = rows.some((row) => row.csvColumn === activeCsvColumn.value)
    if (hasActiveRow) return

    activeCsvColumn.value =
      rows.find((row) => !row.datastreamId)?.csvColumn ?? rows[0].csvColumn
  },
  { immediate: true }
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

function measureDatastreamViewport(): void {
  datastreamViewportHeight.value = datastreamViewportRef.value?.clientHeight ?? 640
}

function onDatastreamScroll(event: Event): void {
  const target = event.target as HTMLElement | null
  datastreamScrollTop.value = target?.scrollTop ?? 0
}

function selectMappingColumn(csvColumn: string): void {
  activeCsvColumn.value = csvColumn
}

function connectDatastream(datastreamId: string): void {
  if (!activeMappingRow.value) return
  model.updatePipelineMappingDatastream(activeMappingRow.value.csvColumn, datastreamId)
}

function clearActiveMapping(): void {
  if (!activeMappingRow.value) return
  model.clearPipelineMapping(activeMappingRow.value.csvColumn)
}

function datastreamTitle(datastream: DatastreamSummary): string {
  return datastream.observed_property_name || datastream.name
}

function datastreamMeta(datastream: DatastreamSummary): string {
  return [
    datastream.processing_level_definition,
    datastream.unit_symbol || datastream.unit_name,
    datastream.sampled_medium,
  ]
    .filter(Boolean)
    .join(" · ")
}

function isDatastreamMappedToActive(datastreamId: string): boolean {
  return activeMappingRow.value?.datastreamId === datastreamId
}

function isDatastreamMappedElsewhere(
  datastreamId: string,
  mappedCsvColumn: string | null
): boolean {
  return Boolean(mappedCsvColumn && mappedCsvColumn !== activeMappingRow.value?.csvColumn)
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

      <div v-else class="mapping-board">
        <aside class="mapping-column-pane">
          <div class="mapping-pane-header">
            <div>
              <p class="mapping-pane-label">CSV columns</p>
              <p class="mapping-pane-copy">
                {{ mappedColumnCount }} of {{ mappingRows.length }} mapped
              </p>
            </div>
            <button
              v-if="activeMappingRow?.datastreamId"
              class="btn-ghost mapping-pane-action"
              type="button"
              @click="clearActiveMapping()"
            >
              Clear selected
            </button>
          </div>

          <div class="mapping-column-list">
            <button
              v-for="row in mappingRows"
              :key="row.csvColumn"
              class="mapping-column-button"
              :class="{
                'mapping-column-button-active':
                  row.csvColumn === activeCsvColumn,
                'mapping-column-button-mapped':
                  row.csvColumn !== activeCsvColumn && !!row.datastreamId,
              }"
              type="button"
              @click="selectMappingColumn(row.csvColumn)"
            >
              <span class="mapping-column-dot" />
              <span class="mapping-column-name">{{ row.label }}</span>
            </button>
          </div>
        </aside>

        <section class="mapping-datastream-pane">
          <div class="mapping-pane-header">
            <div>
              <p class="mapping-pane-label">Datastreams</p>
              <p class="mapping-pane-copy">{{ activeMappingPrompt }}</p>
            </div>
            <div v-if="activeMappingRow" class="mapping-pane-chip">
              {{ activeMappingRow.label }}
            </div>
          </div>

          <div
            ref="datastreamViewportRef"
            class="mapping-datastream-viewport"
            @scroll="onDatastreamScroll"
          >
            <div
              class="mapping-virtual-stage"
              :style="{ height: `${datastreamVirtualHeight}px` }"
            >
              <div
                v-for="entry in visibleDatastreamEntries"
                :key="entry.key"
                class="mapping-virtual-item"
                :style="{
                  transform: `translateY(${entry.top}px)`,
                  height: `${entry.height}px`,
                }"
              >
                <div v-if="entry.kind === 'thing'" class="mapping-thing-header">
                  {{ entry.thingName }}
                </div>

                <button
                  v-else
                  class="mapping-datastream-card"
                  :class="{
                    'mapping-datastream-card-active':
                      isDatastreamMappedToActive(entry.datastream.id),
                    'mapping-datastream-card-occupied':
                      isDatastreamMappedElsewhere(
                        entry.datastream.id,
                        entry.mappedCsvColumn
                      ),
                  }"
                  type="button"
                  @click="connectDatastream(entry.datastream.id)"
                >
                  <div class="mapping-datastream-copy">
                    <p class="mapping-datastream-title">
                      {{ datastreamTitle(entry.datastream) }}
                    </p>
                    <p class="mapping-datastream-meta">
                      {{ datastreamMeta(entry.datastream) }}
                    </p>
                  </div>

                  <div class="mapping-datastream-status">
                    <span
                      v-if="isDatastreamMappedToActive(entry.datastream.id)"
                      class="mapping-linked-badge"
                    >
                      mapped
                    </span>
                    <span v-else-if="entry.mappedColumnLabel" class="mapping-occupied-label">
                      {{ entry.mappedColumnLabel }}
                    </span>
                  </div>
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
    </article>
  </section>
</template>
