<script setup lang="ts">
import { computed } from "vue"

import { routeHref } from "../router"
import { useAppModel } from "../composables/useAppModel"

defineProps<{
  showActions: boolean
}>()

const model = useAppModel()
const isConnected = computed(() => model.isConnected.value)

const datastreamText = computed(() => {
  const count = model.state.connectionSummary?.datastream_count ?? 0
  return count === 1 ? "1 datastream available" : `${count} datastreams available`
})
</script>

<template>
  <article
    v-if="isConnected && model.state.connectionSummary"
    class="summary-card"
  >
    <div class="summary-card-copy">
      <p class="eyebrow">Authenticated</p>
      <h2 class="section-title">
        {{ model.state.connectionSummary.instance_name ?? "HydroServer" }}
      </h2>
      <p class="section-copy">{{ model.state.connectionSummary.message }}</p>
      <div class="summary-inline">
        <span class="pill-success">Connected</span>
        <span class="summary-meta">{{ datastreamText }}</span>
      </div>
    </div>

    <div v-if="showActions" class="button-row">
      <button class="btn-danger" type="button" @click="model.disconnectHydroServer()">
        Disconnect
      </button>
      <button class="btn-ghost" type="button" @click="model.changeCredentials()">
        Change credentials
      </button>
      <a
        v-if="model.state.jobs.length === 0"
        class="btn-primary"
        :href="routeHref('jobs-new')"
      >
        Create first pipeline
      </a>
    </div>
  </article>
</template>
