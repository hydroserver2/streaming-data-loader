<script setup lang="ts">
import type { PipelineFieldName } from "../pipeline-submit";

import CsvPreview from "../components/CsvPreview.vue";
import FeedbackBanner from "../components/FeedbackBanner.vue";
import { useAppModel } from "../composables/useAppModel";

const model = useAppModel();

function fieldError(field: PipelineFieldName): string | null {
  const state = model.state.pipelineFieldStates[field];
  return state.state === "invalid" ? state.message : null;
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
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

      <div class="button-row">
        <button
          class="btn-primary"
          type="button"
          @click="model.browseForCsvPath()"
        >
          Choose CSV File
        </button>
        <button class="btn-danger" type="button" @click="model.disconnectHydroServer()">
          Disconnect
        </button>
      </div>
    </form>

    <FeedbackBanner :feedback="model.state.pipelineFeedback" />
    <CsvPreview />
  </section>
</template>
