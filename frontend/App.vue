<script setup lang="ts">
import { computed, onMounted } from "vue"

import FatalErrorView from "./views/FatalErrorView.vue"
import DashboardView from "./views/DashboardView.vue"
import LoadingView from "./views/LoadingView.vue"
import PipelineEditorView from "./views/PipelineEditorView.vue"
import SettingsView from "./views/SettingsView.vue"
import WelcomeView from "./views/WelcomeView.vue"
import { routeHref } from "./router"
import { useAppModel } from "./composables/useAppModel"

const model = useAppModel()

const jobsLinkClass = computed(() =>
  model.state.route === "dashboard"
    ? "nav-item nav-item-active"
    : "nav-item"
)

const settingsLinkClass = computed(() =>
  model.state.route === "settings"
    ? "nav-item nav-item-active"
    : "nav-item"
)

const connectionIndicator = computed(() => model.connectionIndicator())
const showSidebar = computed(() => model.showSidebar.value)
const useWelcomeSurface = computed(() => model.useWelcomeSurface.value)

onMounted(() => {
  model.init()
})
</script>

<template>
  <div class="flex min-h-screen">
    <nav
      v-if="showSidebar"
      id="app-sidebar"
      class="flex w-16 shrink-0 flex-col items-center gap-2 border-r border-slate-200 bg-white px-3 py-4"
    >
      <div
        class="mb-4 flex h-10 w-10 items-center justify-center rounded-2xl bg-sky-50 text-sky-700"
      >
        <span class="text-sm font-semibold">HS</span>
      </div>

      <a :class="jobsLinkClass" :href="routeHref('dashboard')" title="Jobs">
        <svg viewBox="0 0 24 24" class="h-5 w-5 fill-none stroke-current stroke-[1.8]">
          <rect x="4" y="5" width="16" height="14" rx="2" />
          <path d="M8 9h8M8 13h8M8 17h5" />
        </svg>
      </a>

      <a :class="settingsLinkClass" :href="routeHref('settings')" title="Settings">
        <svg viewBox="0 0 24 24" class="h-5 w-5 fill-none stroke-current stroke-[1.8]">
          <path
            d="M12 3v4M12 17v4M4.9 4.9l2.8 2.8M16.3 16.3l2.8 2.8M3 12h4M17 12h4M4.9 19.1l2.8-2.8M16.3 7.7l2.8-2.8"
          />
          <circle cx="12" cy="12" r="4" />
        </svg>
      </a>

      <div class="mt-auto flex h-12 w-12 items-center justify-center">
        <span :class="connectionIndicator.className" :title="connectionIndicator.label" />
      </div>
    </nav>

    <main
      id="main-content"
      class="min-w-0 flex-1"
      :class="{ 'main-content-welcome': useWelcomeSurface }"
    >
      <LoadingView v-if="model.state.loading" />
      <FatalErrorView v-else-if="model.state.bootstrapError" />
      <SettingsView v-else-if="model.state.route === 'settings'" />
      <WelcomeView v-else-if="model.state.route === 'welcome'" />
      <PipelineEditorView v-else-if="model.state.route === 'jobs-new'" />
      <DashboardView v-else />
    </main>
  </div>
</template>
