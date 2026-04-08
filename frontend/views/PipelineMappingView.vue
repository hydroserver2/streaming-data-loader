<script setup lang="ts">
import { computed, onMounted, watch } from "vue"

import type { DatastreamSummary } from "../api"
import { useAppModel } from "../composables/useAppModel"
import { navigate } from "../router"

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

watch(
  validatedSettings,
  () => {
    model.syncPipelineMappingDrafts()
  },
  { immediate: true }
)

onMounted(() => {
  model.syncPipelineMappingDrafts()
  void model.loadPipelineDatastreams()
})

function datastreamOptions(csvColumn: string, thingId: string): DatastreamSummary[] {
  if (!thingId) return []
  return model.datastreamOptionsForThing(thingId, csvColumn)
}

function datastreamOptionLabel(datastream: DatastreamSummary): string {
  const parts = [
    datastream.observed_property_name || datastream.name,
    datastream.processing_level_definition,
    datastream.unit_symbol || datastream.unit_name,
  ].filter(Boolean)

  return parts.join(" · ")
}

function datastreamTitle(datastream: DatastreamSummary): string {
  return datastream.observed_property_name || datastream.name
}

function unitLabel(datastream: DatastreamSummary): string {
  return datastream.unit_symbol || datastream.unit_name || "No unit"
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
    <header class="page-header wizard-header">
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
    </header>

    <article class="pipeline-subcard">
      <div v-if="model.state.pipelineDatastreamsLoading" class="empty-panel">
        <div class="empty-icon">...</div>
        <p class="section-copy">Loading HydroServer datastreams.</p>
      </div>

      <div
        v-else-if="!hasDatastreamInventory"
        class="empty-panel"
      >
        <div class="empty-icon">0</div>
        <p class="section-copy">
          No datastreams were returned for the connected workspace.
        </p>
      </div>

      <div
        v-else-if="model.pipelineMappingRows.value.length === 0"
        class="empty-panel"
      >
        <div class="empty-icon">TS</div>
        <p class="section-copy">
          No value columns are available to map after excluding the timestamp
          column.
        </p>
      </div>

      <div v-else class="mapping-grid">
        <div
          v-for="row in model.pipelineMappingRows.value"
          :key="row.csvColumn"
          class="mapping-row mapping-row-rich"
        >
          <div class="mapping-source-stack">
            <p class="mapping-source">{{ row.label }}</p>
            <p class="mapping-help">Source column</p>
          </div>

          <div class="mapping-controls">
            <label class="field">
              <span class="label">Thing</span>
              <select
                class="input"
                :value="row.thingId"
                @change="
                  model.updatePipelineMappingThing(
                    row.csvColumn,
                    ($event.target as HTMLSelectElement).value
                  )
                "
              >
                <option value="">Select a thing</option>
                <option
                  v-for="thing in model.pipelineThingOptions.value"
                  :key="thing.id"
                  :value="thing.id"
                >
                  {{ thing.name }}
                </option>
              </select>
            </label>

            <label class="field">
              <span class="label">Datastream</span>
              <select
                class="input"
                :disabled="!row.thingId"
                :value="row.datastreamId"
                @change="
                  model.updatePipelineMappingDatastream(
                    row.csvColumn,
                    ($event.target as HTMLSelectElement).value
                  )
                "
              >
                <option value="">Select a datastream</option>
                <option
                  v-for="datastream in datastreamOptions(row.csvColumn, row.thingId)"
                  :key="datastream.id"
                  :value="datastream.id"
                >
                  {{ datastreamOptionLabel(datastream) }}
                </option>
              </select>
            </label>

            <div class="button-row button-row-tight">
              <button
                class="btn-ghost"
                type="button"
                :disabled="!row.datastreamId && !row.thingId"
                @click="model.clearPipelineMapping(row.csvColumn)"
              >
                Clear
              </button>
            </div>
          </div>

          <div v-if="row.selectedDatastream" class="mapping-summary-card">
            <p class="mapping-summary-title">
              {{ datastreamTitle(row.selectedDatastream) }}
            </p>
            <p class="mapping-help">
              {{ row.selectedDatastream.thing_name }}
            </p>
            <div class="mapping-meta-row">
              <span class="pill-info">
                {{ row.selectedDatastream.processing_level_definition || "No processing level" }}
              </span>
              <span class="pill-muted">{{ unitLabel(row.selectedDatastream) }}</span>
              <span class="pill-muted">
                {{ row.selectedDatastream.sampled_medium || "Unknown medium" }}
              </span>
            </div>
            <p class="mapping-help">
              ID {{ row.selectedDatastream.id }}
            </p>
          </div>

          <div v-else class="mapping-summary-card mapping-summary-card-empty">
            <p class="mapping-help">
              Choose a thing, then select one of its datastreams.
            </p>
          </div>
        </div>
      </div>
    </article>
  </section>
</template>
