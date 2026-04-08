<script setup lang="ts">
import { computed } from "vue"

import FeedbackBanner from "../components/FeedbackBanner.vue"
import { useAppModel } from "../composables/useAppModel"
import { navigate } from "../router"

const model = useAppModel()

const validatedSettings = computed(() => model.state.validatedPipelineSettings)

const previewFileName = computed(
  () =>
    model.state.pipelineForm.filePath.split(/[\\/]/).filter(Boolean).at(-1) ??
    model.state.pipelineForm.filePath
)

const sourceColumns = computed(() =>
  model.previewHeaders.value.map((header, index) => {
    const settings = validatedSettings.value
    const isTimestampColumn =
      settings?.identifierType === "index"
        ? settings.timestamp.key === String(index + 1)
        : settings?.timestamp.key === header

    return {
      id: `${index + 1}-${header}`,
      label: settings?.identifierType === "index" ? `${index + 1} · ${header}` : header,
      help: isTimestampColumn
        ? "Timestamp source column"
        : "Ready for datastream mapping on the next step.",
    }
  })
)
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell">
    <FeedbackBanner :feedback="model.state.pipelineFeedback" />

    <header class="page-header">
      <div>
        <p class="eyebrow">Step 2</p>
        <h1 class="page-title">Column mapping</h1>
        <p class="page-copy">
          The CSV transformer settings validated successfully. Review the source
          columns here before wiring them to HydroServer datastreams.
        </p>
      </div>

      <div class="button-row button-row-end">
        <button class="btn-ghost" type="button" @click="navigate('jobs-new')">
          Back to CSV Setup
        </button>
      </div>
    </header>

    <article class="summary-card">
      <div class="summary-card-copy">
        <p class="eyebrow">Validated file</p>
        <h2 class="section-title">{{ previewFileName }}</h2>
        <div class="summary-inline">
          <span class="summary-meta">
            Delimiter: {{ validatedSettings?.delimiter ?? model.state.pipelineForm.delimiter }}
          </span>
          <span class="summary-meta">
            Data start row:
            {{ validatedSettings?.dataStartRow ?? model.state.pipelineForm.dataStartRow }}
          </span>
          <span class="summary-meta">
            Timestamp:
            {{ validatedSettings?.timestamp.key ?? model.state.pipelineForm.timestamp.key }}
          </span>
        </div>
      </div>
    </article>

    <article class="pipeline-subcard">
      <div class="transformer-section-header">
        <p class="eyebrow">Sources</p>
        <h2 class="section-title">Available columns</h2>
        <p class="section-copy">
          Each source column below is now ready to be mapped.
        </p>
      </div>

      <div class="mapping-grid">
        <div v-for="column in sourceColumns" :key="column.id" class="mapping-row">
          <div>
            <p class="mapping-source">{{ column.label }}</p>
            <p class="mapping-help">{{ column.help }}</p>
          </div>
          <p class="mapping-help">No datastream selected yet.</p>
        </div>
      </div>
    </article>
  </section>
</template>
