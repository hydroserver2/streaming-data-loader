<script setup lang="ts">
import { onMounted } from "vue"

import PipelineEditorView from "./views/PipelineEditorView.vue"
import WelcomeView from "./views/WelcomeView.vue"
import { useAppModel } from "./composables/useAppModel"

const model = useAppModel()

onMounted(() => {
  model.init()
})
</script>

<template>
  <main
    id="main-content"
    class="min-w-0 flex-1"
    :class="{ 'main-content-welcome': model.useWelcomeSurface.value }"
  >
    <section v-if="model.state.loading" class="loading-shell" aria-label="Loading">
      <div class="loading-spinner" />
    </section>

    <section v-else-if="model.state.bootstrapError" class="welcome-shell">
      <div class="welcome-card">
        <p class="eyebrow">Sidecar error</p>
        <h1 class="page-title">The background process is unavailable</h1>
        <p class="page-copy">
          {{ model.state.bootstrapError }}
        </p>
        <button class="btn-primary" type="button" @click="model.bootstrap()">
          Retry
        </button>
      </div>
    </section>

    <WelcomeView v-else-if="model.state.route === 'welcome'" />
    <PipelineEditorView v-else />
  </main>
</template>
