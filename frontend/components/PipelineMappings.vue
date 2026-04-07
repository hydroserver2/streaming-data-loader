<script setup lang="ts">
import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()
</script>

<template>
  <div class="pipeline-subcard">
    <template v-if="model.state.pipelinePreview && model.state.pipelineForm.mappings.length > 0">
      <div>
        <h3 class="section-title">Column mappings</h3>
        <p class="section-copy">
            Map each source column to a HydroServer datastream. Leave unused source
            columns as Not mapped.
        </p>

        <div class="mapping-grid">
          <div
            v-for="mapping in model.state.pipelineForm.mappings"
            :key="mapping.csvColumn"
            class="mapping-row"
          >
            <div>
              <p class="mapping-source">{{ mapping.csvColumn }}</p>
              <p class="mapping-help">Source column</p>
            </div>
            <select
              :value="mapping.datastreamId"
              class="input"
              @change="model.updateMapping(mapping.csvColumn, ($event.target as HTMLSelectElement).value)"
            >
              <option value="">Not mapped</option>
              <option
                v-for="datastream in model.state.datastreams"
                :key="datastream.id"
                :value="datastream.id"
              >
                {{ datastream.name }}
              </option>
            </select>
          </div>
        </div>
      </div>
    </template>

    <template v-else>
      <div>
        <h3 class="section-title">Column mappings</h3>
        <p class="section-copy">
          Load a CSV preview first so Streaming Data Loader can list the available source columns.
        </p>
      </div>
    </template>
  </div>
</template>
