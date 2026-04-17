<script setup lang="ts">
import { onMounted } from "vue"

import DashboardView from "./views/DashboardView.vue"
import PipelineEditorView from "./views/PipelineEditorView.vue"
import PipelineMappingView from "./views/PipelineMappingView.vue"
import ServiceSetupView from "./views/ServiceSetupView.vue"
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

    <WelcomeView v-else-if="model.state.route === 'welcome'" />
    <ServiceSetupView v-else-if="model.state.route === 'service'" />
    <DashboardView v-else-if="model.state.route === 'dashboard'" />
    <PipelineMappingView v-else-if="model.state.route === 'jobs-new-mapping'" />
    <PipelineEditorView v-else />
  </main>
</template>
