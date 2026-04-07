<script setup lang="ts">
import { computed } from "vue"

import { routeHref } from "../router"
import { formatRelativeTime, formatSchedule, shortenPath } from "../time"
import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()

const jobs = computed(() => model.state.jobs)

function statusDotClass(status: string): string {
  switch (status) {
    case "error":
      return "bg-rose-500"
    case "warning":
      return "bg-amber-500"
    case "disabled":
      return "bg-slate-300"
    default:
      return "bg-emerald-500"
  }
}

function statusPillClass(status: string): string {
  switch (status) {
    case "warning":
      return "pill-warning"
    case "error":
      return "pill-danger"
    case "disabled":
      return "pill-muted"
    case "pending":
    case "running":
      return "pill-info"
    default:
      return "pill-success"
  }
}
</script>

<template>
  <section class="page-shell" :class="{ 'animate-fade-in': jobs.length === 0 }">
    <header class="page-header">
      <div>
        <p class="eyebrow">Dashboard</p>
        <h1 class="page-title">
          {{ jobs.length === 0 ? "Jobs" : "Pipelines" }}
        </h1>
        <p class="page-copy">
          <template v-if="jobs.length === 0">
            Finish the onboarding flow by creating your first pipeline. {{ model.APP_NAME }} will
            use that saved local configuration from then on.
          </template>
          <template v-else>
            Your saved pipelines watch local CSV sources, track row cursors, and push only new
            observations into HydroServer.
          </template>
        </p>
      </div>

      <a class="btn-primary" :href="routeHref('jobs-new')">
        {{ jobs.length === 0 ? "Create first pipeline" : "Add pipeline" }}
      </a>
    </header>

    <div v-if="jobs.length > 0" class="card-stack">
      <article
        v-for="job in jobs"
        :key="job.id"
        class="job-card animate-fade-in"
      >
        <div class="job-card-top">
          <div>
            <div class="job-card-title-row">
              <span class="status-dot" :class="statusDotClass(job.status)" />
              <h2 class="section-title">{{ job.name }}</h2>
            </div>
            <p class="section-copy">{{ shortenPath(job.file_path) }}</p>
            <p class="job-meta" :class="{ 'text-rose-600': job.status === 'error' }">
              {{
                job.last_error
                  ? `Failed ${formatRelativeTime(job.last_run_at)}`
                  : `Last pushed ${formatRelativeTime(job.last_pushed_timestamp)}`
              }}
              · {{ formatSchedule(job.schedule_minutes) }}
            </p>
          </div>

          <span :class="statusPillClass(job.status)">{{ job.status_message }}</span>
        </div>

        <div class="job-card-actions">
          <button class="btn-ghost" type="button" @click="model.handleRunJob(job.id)">
            Run now
          </button>
          <button class="btn-ghost" type="button" @click="model.handleToggleJob(job.id)">
            {{ job.enabled ? "Disable" : "Enable" }}
          </button>
          <button class="btn-danger" type="button" @click="model.handleDeleteJob(job.id)">
            Delete
          </button>
        </div>
      </article>
    </div>
  </section>
</template>
