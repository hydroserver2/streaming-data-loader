<script setup lang="ts">
import { computed } from 'vue'

import type { JobConfig } from '../api'
import AccountMenuButton from '../components/AccountMenuButton.vue'
import FeedbackBanner from '../components/FeedbackBanner.vue'
import { useAppModel } from '../composables/useAppModel'
import { navigate } from '../router'

const model = useAppModel()

const jobs = computed(() => model.state.config?.jobs ?? [])
const workspaceLabel = computed(
  () =>
    model.state.connectionSummary?.workspace_name?.trim() ||
    model.state.connectionSummary?.workspace_id?.trim() ||
    model.state.config?.server.workspace_id ||
    'Current workspace'
)
const datasourceCountLabel = computed(() =>
  jobs.value.length === 1 ? '1 source' : `${jobs.value.length} sources`
)

function mappingCount(job: JobConfig): number {
  return job.column_mappings.length
}
</script>

<template>
  <section
    class="page-shell animate-fade-in onboarding-shell pipeline-editor-shell"
  >
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <h1 class="wizard-page-title">{{ workspaceLabel }}</h1>
          <p class="mapping-help">{{ datasourceCountLabel }}</p>
        </div>
        <div class="button-row wizard-actions">
          <button
            class="btn-primary wizard-nav-button"
            type="button"
            @click="navigate('jobs-new')"
          >
            + Add Data Source
          </button>
          <AccountMenuButton />
        </div>
      </div>
    </header>

    <div class="flex flex-col gap-4">
      <FeedbackBanner :feedback="model.state.pipelineCreateFeedback" />

      <div class="flex flex-col gap-2">
        <article
          v-for="job in jobs"
          :key="job.id"
          class="rounded-2xl bg-[#111315] px-5 py-4"
        >
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="mapping-source-stack min-w-0">
              <p class="mapping-summary-title">{{ job.name }}</p>
              <p class="mapping-help break-all">{{ job.file_path }}</p>
            </div>
            <p class="mapping-help whitespace-nowrap">
              <span
                :class="job.enabled ? 'text-emerald-300' : 'text-slate-500'"
              >
                {{ job.enabled ? 'Enabled' : 'Paused' }}
              </span>
              ·
              {{ mappingCount(job) }}
              {{ mappingCount(job) === 1 ? 'mapping' : 'mappings' }}
            </p>
          </div>
        </article>

        <article
          v-if="jobs.length === 0"
          class="rounded-2xl bg-[#111315] px-5 py-6"
        >
          <p class="mapping-help">No data sources yet.</p>
        </article>
      </div>
    </div>
  </section>
</template>
