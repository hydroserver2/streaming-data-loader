<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()

const isOpen = ref(false)
const menuRef = ref<HTMLElement | null>(null)

const serverUrl = computed(() => model.state.config?.server.url?.trim() ?? "")
const instanceName = computed(
  () => model.state.connectionSummary?.instance_name?.trim() || "HydroServer account"
)
const authModeLabel = computed(() =>
  model.state.authDraft.auth_type === "userpass" ? "Username + password" : "API key"
)
const workspaceLabel = computed(
  () =>
    model.state.config?.server.workspace_id ||
    model.state.authDraft.workspace_id ||
    "No workspace"
)
const hostLabel = computed(() => {
  if (!serverUrl.value) return "Not connected"

  try {
    return new URL(serverUrl.value).host
  } catch {
    return serverUrl.value.replace(/^https?:\/\//, "")
  }
})

function toggleMenu(): void {
  isOpen.value = !isOpen.value
}

function closeMenu(): void {
  isOpen.value = false
}

function onDocumentPointerDown(event: PointerEvent): void {
  const target = event.target as Node | null
  if (!target || menuRef.value?.contains(target)) return
  closeMenu()
}

function onDocumentKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    closeMenu()
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onDocumentPointerDown)
  document.addEventListener("keydown", onDocumentKeydown)
})

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocumentPointerDown)
  document.removeEventListener("keydown", onDocumentKeydown)
})
</script>

<template>
  <div ref="menuRef" class="account-menu">
    <button
      class="account-menu-button"
      type="button"
      aria-label="Account"
      :aria-expanded="isOpen"
      @click="toggleMenu()"
    >
      <svg
        class="account-menu-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="1.8"
        aria-hidden="true"
      >
        <path d="M18 20a6 6 0 0 0-12 0" />
        <circle cx="12" cy="8" r="4" />
      </svg>
    </button>

    <section v-if="isOpen" class="account-menu-panel">
      <div class="account-menu-copy">
        <p class="account-menu-eyebrow">Account</p>
        <h2 class="account-menu-title">{{ instanceName }}</h2>
        <p class="account-menu-meta">{{ hostLabel }}</p>
      </div>

      <dl class="account-menu-details">
        <div class="account-menu-detail">
          <dt>Authentication</dt>
          <dd>{{ authModeLabel }}</dd>
        </div>
        <div class="account-menu-detail">
          <dt>Workspace</dt>
          <dd>{{ workspaceLabel }}</dd>
        </div>
      </dl>

      <div class="account-menu-actions">
        <button class="btn-danger account-menu-disconnect" type="button" @click="model.disconnectHydroServer()">
          Disconnect
        </button>
      </div>
    </section>
  </div>
</template>
