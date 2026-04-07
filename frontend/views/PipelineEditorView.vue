<script setup lang="ts">
import { computed } from "vue"

import ConnectedCard from "../components/ConnectedCard.vue"
import CsvPreview from "../components/CsvPreview.vue"
import FeedbackBanner from "../components/FeedbackBanner.vue"
import PipelineMappings from "../components/PipelineMappings.vue"
import { API_KEY_DOCS_URL, APP_NAME, useAppModel } from "../composables/useAppModel"

const model = useAppModel()
const isFirstRunOnboarding = computed(() => model.state.jobs.length === 0)
</script>

<template>
  <section
    class="page-shell animate-fade-in"
    :class="{ 'onboarding-shell': isFirstRunOnboarding }"
  >
    <template v-if="isFirstRunOnboarding">
      <form
        id="pipeline-form"
        class="onboarding-file-form"
        autocomplete="off"
        @submit.prevent="model.submitPipeline()"
      >
        <label class="field">
          <span class="label">CSV file path</span>
          <input
            :value="model.state.pipelineForm.filePath"
            class="input"
            type="text"
            placeholder="/Users/you/datalogger/output.csv"
            @input="model.updatePipelineField('file_path', ($event.target as HTMLInputElement).value)"
            @change="model.loadPipelinePreview(($event.target as HTMLInputElement).value)"
          />
          <span class="field-hint">
            Select a CSV file from your system to load a preview and start configuring this data
            source.
          </span>
        </label>

        <div class="button-row">
          <button class="btn-primary" type="button" @click="model.browseForCsvPath()">
            Choose CSV File
          </button>
        </div>
      </form>

      <FeedbackBanner :feedback="model.state.pipelineFeedback" />
      <CsvPreview v-if="model.state.pipelinePreview" />
    </template>

    <template v-else-if="model.state.datastreamsError">
      <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">HydroServer access needs attention</h1>
          <p class="page-copy">
          {{ APP_NAME }} authenticated successfully, but it could not load the target datastreams
          needed for mapping.
        </p>
        </div>
      </header>

      <ConnectedCard :show-actions="true" />
      <FeedbackBanner :feedback="{ tone: 'error', message: model.state.datastreamsError }" />
    </template>

    <template v-else-if="model.state.datastreams.length === 0">
      <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">No datastreams are available yet</h1>
          <p class="page-copy">
          Create at least one target datastream in HydroServer first, then come back and
          {{ APP_NAME }} will use it for column mapping.
        </p>
        </div>
      </header>

      <ConnectedCard :show-actions="true" />
      <a
        class="btn-link"
        :href="API_KEY_DOCS_URL"
        target="_blank"
        rel="noreferrer"
      >
        Open the HydroServer 101 tutorial
      </a>
    </template>

    <template v-else>
      <header class="page-header">
        <div>
          <p class="eyebrow">Create first pipeline</p>
          <h1 class="page-title">Connect a CSV source to HydroServer</h1>
          <p class="page-copy">
          Choose the CSV file you want {{ APP_NAME }} to watch, preview the first 50 lines, then
          click the structure on the right to fill the setup form on the left.
        </p>
        </div>
      </header>

      <ConnectedCard :show-actions="true" />

      <div class="pipeline-layout">
        <form
          id="pipeline-form"
          class="pipeline-form"
          autocomplete="off"
          @submit.prevent="model.submitPipeline()"
        >
          <div class="pipeline-subcard">
            <h2 class="section-title">Pipeline details</h2>

            <label class="field">
              <span class="label">Pipeline name</span>
              <input
                :value="model.state.pipelineForm.name"
                class="input"
                type="text"
                placeholder="Little Bear River stage"
                @input="model.updatePipelineField('pipeline_name', ($event.target as HTMLInputElement).value)"
              />
            </label>

            <label class="field">
              <span class="label">Watched CSV file path</span>
              <input
                :value="model.state.pipelineForm.filePath"
                class="input"
                type="text"
                placeholder="/Users/you/datalogger/output.csv"
                @input="model.updatePipelineField('file_path', ($event.target as HTMLInputElement).value)"
                @change="model.loadPipelinePreview(($event.target as HTMLInputElement).value)"
              />
              <span class="field-hint">
                {{ APP_NAME }} stores the watched file path locally so it can keep loading new
                rows in the background.
              </span>
            </label>

            <div class="button-row">
              <button class="btn-primary" type="button" @click="model.browseForCsvPath()">
                Choose CSV File
              </button>
            </div>

            <label class="field">
              <span class="label">Schedule</span>
              <select
                :value="model.state.pipelineForm.scheduleMinutes"
                class="input"
                @change="model.updatePipelineField('schedule_minutes', ($event.target as HTMLSelectElement).value)"
              >
                <option v-for="minutes in [5, 15, 30, 60]" :key="minutes" :value="minutes">
                  Every {{ minutes }} minutes
                </option>
              </select>
            </label>
          </div>

          <div class="pipeline-subcard">
            <h2 class="section-title">File structure</h2>

            <div class="split-fields">
              <template v-if="model.state.pipelineForm.hasHeaderRow">
                <div :class="model.previewFieldClass('header-row')">
                  <div class="field-label-row">
                    <label class="label" for="pipeline-header-row">Header row</label>
                  </div>
                  <input
                    id="pipeline-header-row"
                    :value="model.state.pipelineForm.headerRow"
                    class="input"
                    type="number"
                    min="1"
                    @input="model.updatePipelineField('header_row', ($event.target as HTMLInputElement).value)"
                  />
                  <span class="field-hint">
                    Drag the blue HEADER handle in the preview or enter a row number.
                  </span>
                </div>
              </template>

              <template v-else>
                <div class="field">
                  <span class="label">Header row</span>
                  <span class="field-hint">
                    This file is using generated column labels: Column 1, Column 2, Column 3...
                  </span>
                </div>
              </template>

              <div :class="model.previewFieldClass('data-start-row')">
                <div class="field-label-row">
                  <label class="label" for="pipeline-data-start-row">Data start row</label>
                </div>
                <input
                  id="pipeline-data-start-row"
                  :value="model.state.pipelineForm.dataStartRow"
                  class="input"
                  type="number"
                  :min="model.state.pipelineForm.hasHeaderRow ? 2 : 1"
                  @input="model.updatePipelineField('data_start_row', ($event.target as HTMLInputElement).value)"
                />
                <span class="field-hint">
                  Drag the green DATA START handle in the preview or enter a row number.
                </span>
              </div>
            </div>

            <div class="split-fields">
              <label class="field">
                <span class="label">Delimiter</span>
                <input
                  :value="model.state.pipelineForm.delimiter"
                  class="input"
                  type="text"
                  maxlength="2"
                  @input="model.updatePipelineField('delimiter', ($event.target as HTMLInputElement).value)"
                />
              </label>

              <label class="field">
                <span class="label">Timezone</span>
                <input
                  :value="model.state.pipelineForm.timezone"
                  class="input"
                  type="text"
                  @input="model.updatePipelineField('timezone', ($event.target as HTMLInputElement).value)"
                />
              </label>
            </div>

            <div :class="model.previewFieldClass('timestamp-column')">
              <div class="field-label-row">
                <label class="label" for="pipeline-timestamp-column">Timestamp column</label>
              </div>
                <select
                  v-if="model.previewHeaders.value.length > 0"
                  id="pipeline-timestamp-column"
                  :value="model.state.pipelineForm.timestampColumn"
                  class="input"
                  @change="model.updatePipelineField('timestamp_column', ($event.target as HTMLSelectElement).value)"
                >
                  <option
                    v-for="header in model.previewHeaders.value"
                    :key="header"
                    :value="header"
                  >
                    {{ header }}
                  </option>
                </select>
                <input
                  v-else
                  id="pipeline-timestamp-column"
                  :value="model.state.pipelineForm.timestampColumn"
                  class="input"
                  type="text"
                  placeholder="Timestamp"
                  @input="model.updatePipelineField('timestamp_column', ($event.target as HTMLInputElement).value)"
                />
              <span class="field-hint">
                Drag the amber TIMESTAMP handle in the preview, or click the matching header.
              </span>
            </div>

            <label class="field">
              <span class="label">Timestamp format</span>
              <input
                :value="model.state.pipelineForm.timestampFormat"
                class="input"
                type="text"
                placeholder="%Y-%m-%d %H:%M:%S"
                @input="model.updatePipelineField('timestamp_format', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>

          <PipelineMappings />

          <div v-if="model.state.pipelineErrors.length > 0" class="validation-panel">
            <h3 class="section-title">Fix these issues before saving</h3>
            <ul class="validation-list">
              <li v-for="error in model.state.pipelineErrors" :key="error">{{ error }}</li>
            </ul>
          </div>

          <FeedbackBanner :feedback="model.state.pipelineFeedback" />

          <div class="button-row">
            <button class="btn-primary" type="submit">Save pipeline</button>
          </div>
        </form>

        <CsvPreview />
      </div>
    </template>
  </section>
</template>
