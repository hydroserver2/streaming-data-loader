<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()

const isOpen = ref(false)
const menuRef = ref<HTMLElement | null>(null)

const serviceStatusLabel = computed(() => {
  const status = model.state.serviceStatus
  if (!status?.supported) return "Unavailable"
  if (status.installed && status.running) return "Installed and running"
  if (status.installed) return "Installed but stopped"
  return "Not installed"
})

const uninstallDisabled = computed(
  () =>
    model.state.serviceActionSubmitting ||
    !model.state.serviceStatus?.supported ||
    !model.state.serviceStatus?.installed
)

const restartDisabled = computed(
  () =>
    model.state.serviceActionSubmitting ||
    !model.state.serviceStatus?.supported ||
    !model.state.serviceStatus?.installed
)

function toggleMenu(): void {
  isOpen.value = !isOpen.value
}

function closeMenu(): void {
  isOpen.value = false
}

async function handleUninstall(): Promise<void> {
  await model.uninstallBackgroundService()
  closeMenu()
}

async function handleRestart(): Promise<void> {
  await model.restartBackgroundService()
  closeMenu()
}

function onDocumentPointerDown(event: PointerEvent): void {
  const target = event.target as Node | null
  if (!target || menuRef.value?.contains(target)) return
  closeMenu()
}

function onDocumentKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") closeMenu()
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
      aria-label="Settings"
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
        <circle cx="12" cy="12" r="3.5" />
        <path
          d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 0 1 0 2.8 2 2 0 0 1-2.8 0l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 0 1-2.8 0 2 2 0 0 1 0-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 0 1 0-2.8 2 2 0 0 1 2.8 0l.1.1a1 1 0 0 0 1.1.2H9a1 1 0 0 0 .6-.9V4a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.2a1 1 0 0 0 .6.9 1 1 0 0 0 1.1-.2l.1-.1a2 2 0 0 1 2.8 0 2 2 0 0 1 0 2.8l-.1.1a1 1 0 0 0-.2 1.1V9c0 .4.2.7.6.9H20a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.2a1 1 0 0 0-.9.6Z"
        />
      </svg>
    </button>

    <section v-if="isOpen" class="account-menu-panel">
      <div class="account-menu-copy">
        <p class="account-menu-eyebrow">Settings</p>
        <h2 class="account-menu-title">Background Service</h2>
      </div>

      <dl class="account-menu-details">
        <div class="account-menu-detail">
          <dt>Status</dt>
          <dd>{{ serviceStatusLabel }}</dd>
        </div>
      </dl>

      <div class="account-menu-actions">
        <button
          class="btn-ghost account-menu-save"
          type="button"
          :disabled="restartDisabled"
          @click="handleRestart()"
        >
          Restart Service
        </button>
        <button
          class="btn-danger account-menu-disconnect"
          type="button"
          :disabled="uninstallDisabled"
          @click="handleUninstall()"
        >
          {{
            model.state.serviceActionSubmitting
              ? "Uninstalling..."
              : "Uninstall Background Service"
          }}
        </button>
      </div>
    </section>
  </div>
</template>
