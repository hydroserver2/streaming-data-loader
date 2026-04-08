<script setup lang="ts">
import { computed } from "vue";

import type { PipelineFieldName } from "../pipeline-submit";

import CsvPreview from "../components/CsvPreview.vue";
import { useAppModel } from "../composables/useAppModel";

const model = useAppModel();

const wizardTitle = computed(() =>
  model.state.pipelinePreview
    ? "Data source creation step 2/3 - CSV setup"
    : "Data source creation step 1/3 - select file"
);

function fieldError(field: PipelineFieldName): string | null {
  const state = model.state.pipelineFieldStates[field];
  return state.state === "invalid" ? state.message : null;
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <h1 class="page-title">{{ wizardTitle }}</h1>
        </div>
        <div class="button-row wizard-actions">
          <button
            class="btn-primary"
            type="button"
            @click="model.browseForCsvPath()"
          >
            Choose CSV File
          </button>
          <button
            v-if="model.state.pipelinePreview"
            class="btn-primary"
            type="button"
            @click="model.submitPipelineConfig()"
          >
            Validate and Continue
          </button>
          <button class="btn-danger" type="button" @click="model.disconnectHydroServer()">
            Disconnect
          </button>
        </div>
      </div>
    </header>

    <form
      id="pipeline-form"
      class="onboarding-file-form"
      autocomplete="off"
      @submit.prevent
    >
      <label class="field">
        <span class="label">CSV file path</span>
        <input
          :value="model.state.pipelineForm.filePath"
          class="input"
          type="text"
          placeholder="/Users/you/datalogger/output.csv"
          @input="
            model.updatePipelineField(
              'file_path',
              ($event.target as HTMLInputElement).value
            )
          "
          @change="
            model.loadPipelinePreview(
              ($event.target as HTMLInputElement).value
            )
          "
        />
        <span class="field-hint">
          Select a CSV file from your system to load a preview and start
          configuring this data source.
        </span>
        <p v-if="fieldError('file_path')" class="field-error">
          {{ fieldError("file_path") }}
        </p>
      </label>

    </form>
    <CsvPreview />
  </section>
</template>
