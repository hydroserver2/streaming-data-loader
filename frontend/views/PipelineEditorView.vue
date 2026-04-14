<script setup lang="ts">
import { computed, ref, watch } from "vue";

import type { PipelineFieldName } from "../pipeline-submit";

import AccountMenuButton from "../components/AccountMenuButton.vue";
import CsvPreview from "../components/CsvPreview.vue";
import { useAppModel } from "../composables/useAppModel";

const model = useAppModel();
type PipelineEditorStep = 1 | 2;

const editorStep = ref<PipelineEditorStep>(
  model.state.pipelineEditorStartStep ?? (model.state.pipelinePreview ? 2 : 1)
);
const hasPreview = computed(() => Boolean(model.state.pipelinePreview));
const isEditing = computed(() => Boolean(model.state.pipelineEditTarget));

watch(
  () => model.state.pipelinePreview,
  (preview) => {
    if (!preview) {
      editorStep.value = 1;
    }
  }
);

const wizardStepLabel = computed(
  () =>
    `${isEditing.value ? "Edit Data Source" : "Data Source Creation"} · Step ${editorStep.value} of 3`
);

const wizardTitle = computed(() =>
  editorStep.value === 1
    ? isEditing.value
      ? "Edit Source File"
      : "Select CSV File"
    : isEditing.value
      ? "Edit CSV Setup"
      : "Configure CSV Import"
);
const canReturnToDashboard = computed(() => model.hasSavedDatasources.value);

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
  <section
    class="page-shell animate-fade-in onboarding-shell pipeline-editor-shell"
    :class="{ 'pipeline-editor-shell-fullscreen': editorStep === 2 }"
  >
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <p class="wizard-step-label">{{ wizardStepLabel }}</p>
          <h1 class="wizard-page-title">{{ wizardTitle }}</h1>
        </div>
        <div class="button-row wizard-actions">
          <button
            v-if="canReturnToDashboard && editorStep === 1"
            class="btn-ghost wizard-nav-button"
            type="button"
            @click="model.abandonPipelineCreation()"
          >
            <span class="wizard-nav-glyph" aria-hidden="true">←</span>
            <span>Back to Dashboard</span>
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
            v-if="editorStep === 2 && isEditing"
            class="btn-ghost wizard-nav-button"
            type="button"
            @click="model.abandonPipelineCreation()"
          >
            Cancel
          </button>
          <button
            v-if="editorStep === 2"
            class="btn-ghost wizard-nav-button"
            type="button"
            @click="goToSourceStep()"
          >
            <span class="wizard-nav-glyph" aria-hidden="true">←</span>
            <span>Source File</span>
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
          <AccountMenuButton />
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
        class="onboarding-file-form pipeline-subcard transformer-section pipeline-source-card"
        autocomplete="off"
        @submit.prevent
      >
        <div class="transformer-section-body">
          <label class="field pipeline-source-field">
            <span class="label">CSV file path</span>
            <div class="pipeline-source-control">
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
                  loadPreviewFromPath(($event.target as HTMLInputElement).value)
                "
              />
              <button
                class="btn-primary pipeline-source-browse"
                type="button"
                @click="browseForCsvPath()"
              >
                Choose CSV File
              </button>
            </div>
            <span class="field-hint">
              Browse from your files or type an absolute path.
            </span>
            <p v-if="fieldError('file_path')" class="field-error">
              {{ fieldError("file_path") }}
            </p>
          </label>
        </div>
      </form>

      <CsvPreview v-else />
    </div>
  </section>
</template>
