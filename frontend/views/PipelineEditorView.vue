<script setup lang="ts">
import { computed, ref, watch } from "vue";

import type { PipelineFieldName } from "../pipeline-submit";

import CsvPreview from "../components/CsvPreview.vue";
import { useAppModel } from "../composables/useAppModel";

const model = useAppModel();
type PipelineEditorStep = 1 | 2;

const editorStep = ref<PipelineEditorStep>(
  model.state.pipelinePreview ? 2 : 1
);
const hasPreview = computed(() => Boolean(model.state.pipelinePreview));

watch(
  () => model.state.pipelinePreview,
  (preview) => {
    if (!preview) {
      editorStep.value = 1;
    }
  }
);

const wizardStepLabel = computed(() =>
  `Data Source Creation · Step ${editorStep.value} of 3`
);

const wizardTitle = computed(() =>
  editorStep.value === 1 ? "Select CSV File" : "Configure CSV Import"
);

function fieldError(field: PipelineFieldName): string | null {
  const state = model.state.pipelineFieldStates[field];
  return state.state === "invalid" ? state.message : null;
}

async function browseForCsvPath(): Promise<void> {
  await model.browseForCsvPath();
  if (model.state.pipelinePreview) {
    editorStep.value = 2;
  }
}

async function loadPreviewFromPath(path: string): Promise<void> {
  await model.loadPipelinePreview(path);
  if (model.state.pipelinePreview) {
    editorStep.value = 2;
  }
}

function goToSourceStep(): void {
  editorStep.value = 1;
}

function goToSetupStep(): void {
  if (model.state.pipelinePreview) {
    editorStep.value = 2;
  }
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <p class="wizard-step-label">{{ wizardStepLabel }}</p>
          <h1 class="wizard-page-title">{{ wizardTitle }}</h1>
        </div>
        <div class="button-row wizard-actions">
          <button
            v-if="editorStep === 1"
            class="btn-primary"
            type="button"
            @click="browseForCsvPath()"
          >
            Choose CSV File
          </button>
          <button
            v-if="editorStep === 1 && hasPreview"
            class="btn-ghost wizard-nav-button"
            type="button"
            @click="goToSetupStep()"
          >
            <span>Review CSV Setup</span>
            <span class="wizard-nav-glyph" aria-hidden="true">→</span>
          </button>
          <button
            v-if="editorStep === 2"
            class="btn-ghost wizard-nav-button"
            type="button"
            @click="goToSourceStep()"
          >
            <span class="wizard-nav-glyph" aria-hidden="true">←</span>
            <span>Back</span>
          </button>
          <button
            v-if="editorStep === 2"
            class="btn-primary wizard-nav-button"
            type="button"
            @click="model.submitPipelineConfig()"
          >
            <span>Validate and Continue</span>
            <span class="wizard-nav-glyph" aria-hidden="true">→</span>
          </button>
          <button class="btn-danger" type="button" @click="model.disconnectHydroServer()">
            Disconnect
          </button>
        </div>
      </div>
    </header>

    <div
      class="pipeline-editor-workspace"
      :class="{ 'pipeline-editor-workspace-empty': editorStep === 1 }"
    >
      <form
        v-if="editorStep === 1"
        id="pipeline-form"
        class="onboarding-file-form pipeline-source-card"
        autocomplete="off"
        @submit.prevent
      >
        <div class="pipeline-source-header">
          <div>
            <p class="pipeline-source-eyebrow">Source file</p>
            <h2 class="pipeline-source-title">CSV input</h2>
          </div>
          <p class="pipeline-source-copy">
            Choose or paste a CSV path to load a preview and configure parsing
            before mapping columns to datastreams.
          </p>
        </div>

        <label class="field pipeline-source-field">
          <span class="label">CSV file path</span>
          <input
            :value="model.state.pipelineForm.filePath"
            class="input pipeline-source-input"
            type="text"
            placeholder="/Users/you/datalogger/output.csv"
            @input="
              model.updatePipelineField(
                'file_path',
                ($event.target as HTMLInputElement).value
              )
            "
            @change="
              loadPreviewFromPath(
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">
            Use the file chooser or paste an absolute path.
          </span>
          <p v-if="fieldError('file_path')" class="field-error">
            {{ fieldError("file_path") }}
          </p>
        </label>
      </form>

      <CsvPreview v-else />
    </div>
  </section>
</template>
