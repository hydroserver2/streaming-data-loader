<template>
  <div class="flex h-screen bg-base-100" :data-theme="theme">

    <!-- Sidebar -->
    <aside class="w-52 flex-shrink-0 bg-base-200 border-r border-base-300 flex flex-col">
      <div class="px-4 py-5 border-b border-base-300">
        <h1 class="font-semibold text-base-content text-sm leading-tight">
          Streaming<br/>Data Loader
        </h1>
      </div>

      <nav class="flex-1 px-2 py-3 flex flex-col gap-1">
        <RouterLink
          v-for="link in navLinks"
          :key="link.to"
          :to="link.to"
          class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm text-base-content/70 hover:bg-base-300 hover:text-base-content transition-colors"
          active-class="bg-base-300 text-base-content font-medium"
        >
          <component :is="link.icon" class="w-4 h-4 flex-shrink-0" />
          {{ link.label }}
        </RouterLink>
      </nav>

      <div class="px-4 py-3 border-t border-base-300 flex items-center gap-2">
        <div
          class="w-2 h-2 rounded-full flex-shrink-0"
          :class="sidecarOnline ? 'bg-success' : 'bg-error'"
        />
        <span class="text-xs text-base-content/50">
          {{ sidecarOnline ? "Connected" : "Offline" }}
        </span>
      </div>
    </aside>

    <!-- Main content -->
    <main class="flex-1 overflow-y-auto">
      <RouterView />
    </main>

  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, h } from "vue"
import { RouterLink, RouterView } from "vue-router"
import axios from "axios"

// ── Theme ─────────────────────────────────────────────────────────────────────
const theme = ref(
  window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
)
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
  theme.value = e.matches ? "dark" : "light"
})

// ── Sidecar status ────────────────────────────────────────────────────────────
const sidecarOnline = ref(false)

onMounted(async () => {
  try {
    await axios.get("http://127.0.0.1:5321/health")
    sidecarOnline.value = true
  } catch {
    sidecarOnline.value = false
  }
})

// ── Nav icons (inline SVG via render functions to avoid icon library dep) ─────
const IconPlug = () => h("svg", { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2" }, [
  h("path", { d: "M18 6L6 18M6 6l12 12" }),
  h("path", { d: "M5 12H3m4-7l-2-2m14 2l2-2M5 12a7 7 0 0 0 7 7m0-14a7 7 0 0 1 7 7" }),
])

const IconConnections = () => h("svg", { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2" }, [
  h("circle", { cx: "12", cy: "5", r: "2" }),
  h("circle", { cx: "5", cy: "19", r: "2" }),
  h("circle", { cx: "19", cy: "19", r: "2" }),
  h("path", { d: "M12 7v4M8.5 17.5l3-3.5M15.5 17.5l-3-3.5" }),
])

const IconTasks = () => h("svg", { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2" }, [
  h("path", { d: "M9 11l3 3L22 4" }),
  h("path", { d: "M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" }),
])

const navLinks = [
  { to: "/connections", label: "Connections", icon: IconConnections },
  { to: "/tasks", label: "Tasks", icon: IconTasks },
]
</script>