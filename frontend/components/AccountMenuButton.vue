<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()

const isOpen = ref(false)
const showApiKeyEditor = ref(false)
const showUserpassEditor = ref(false)
const menuRef = ref<HTMLElement | null>(null)

const instanceName = computed(
  () => model.state.connectionSummary?.instance_name?.trim() || "HydroServer account"
)
const authModeLabel = computed(() =>
  model.state.authDraft.auth_type === "userpass" ? "Username + password" : "API key"
)
const workspaceLabel = computed(
  () =>
    model.state.connectionSummary?.workspace_name?.trim() ||
    model.state.authDraft.workspace_name?.trim() ||
    model.state.connectionSummary?.workspace_id?.trim() ||
    model.state.config?.server.workspace_id ||
    model.state.authDraft.workspace_id ||
    "No workspace"
)
const submitLabelText = computed(() =>
  model.state.authSubmitting ? "Saving..." : "Save account changes"
)

function fieldError(
  name: "api_key" | "username" | "password" | "workspace_name"
): string | null {
  const fieldState = model.state.authFieldStates[name]
  return fieldState.state === "invalid" ? fieldState.message : null
}

function toggleMenu(): void {
  isOpen.value = !isOpen.value
}

function closeMenu(): void {
  isOpen.value = false
  showApiKeyEditor.value = false
  showUserpassEditor.value = false
}

function cancelApiKeyEdit(): void {
  model.updateAuthDraftField(
    "settings-form",
    "api_key",
    model.state.config?.server.api_key ?? ""
  )
  showApiKeyEditor.value = false
}

function cancelUserpassEdit(): void {
  const server = model.state.config?.server
  model.state.authDraft.username = server?.username ?? ""
  model.state.authDraft.password = server?.password ?? ""
  model.state.authDraft.workspace_name = server?.workspace_name ?? ""
  model.state.authDraft.workspace_id = server?.workspace_id ?? ""
  model.state.authFieldStates.username = { state: "idle", message: null }
  model.state.authFieldStates.password = { state: "idle", message: null }
  model.state.authFieldStates.workspace_name = { state: "idle", message: null }
  showUserpassEditor.value = false
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

      <form class="account-menu-form" autocomplete="off" @submit.prevent="model.submitAuthConfig('settings-form')">
        <template v-if="model.state.authDraft.auth_type === 'apikey'">
          <button
            v-if="!showApiKeyEditor"
            class="btn-ghost account-menu-inline-action"
            type="button"
            @click="showApiKeyEditor = true"
          >
            Change API key
          </button>

          <label v-else class="field account-menu-field">
            <span class="label">API key</span>
            <input
              :value="model.state.authDraft.api_key"
              class="input account-menu-input"
              type="password"
              placeholder="Enter a new API key"
              @input="model.updateAuthDraftField('settings-form', 'api_key', ($event.target as HTMLInputElement).value)"
            />
            <p v-if="fieldError('api_key')" class="field-error">
              {{ fieldError("api_key") }}
            </p>
          </label>
        </template>

        <template v-else>
          <button
            v-if="!showUserpassEditor"
            class="btn-ghost account-menu-inline-action"
            type="button"
            @click="showUserpassEditor = true"
          >
            Update account credentials
          </button>

          <template v-else>
            <label class="field account-menu-field">
              <span class="label">Username</span>
              <input
                :value="model.state.authDraft.username"
                class="input account-menu-input"
                type="text"
                placeholder="name@example.com"
                @input="model.updateAuthDraftField('settings-form', 'username', ($event.target as HTMLInputElement).value)"
              />
              <p v-if="fieldError('username')" class="field-error">
                {{ fieldError("username") }}
              </p>
            </label>

            <label class="field account-menu-field">
              <span class="label">Password</span>
              <input
                :value="model.state.authDraft.password"
                class="input account-menu-input"
                type="password"
                placeholder="Enter your HydroServer password"
                @input="model.updateAuthDraftField('settings-form', 'password', ($event.target as HTMLInputElement).value)"
              />
              <p v-if="fieldError('password')" class="field-error">
                {{ fieldError("password") }}
              </p>
            </label>

            <label class="field account-menu-field">
              <span class="label">Workspace name</span>
              <input
                :value="model.state.authDraft.workspace_name"
                class="input account-menu-input"
                type="text"
                placeholder="Workspace"
                @input="model.updateAuthDraftField('settings-form', 'workspace_name', ($event.target as HTMLInputElement).value)"
              />
              <p v-if="fieldError('workspace_name')" class="field-error">
                {{ fieldError("workspace_name") }}
              </p>
            </label>
          </template>
        </template>

        <div class="account-menu-actions">
          <button
            v-if="(model.state.authDraft.auth_type === 'apikey' && showApiKeyEditor) || (model.state.authDraft.auth_type !== 'apikey' && showUserpassEditor)"
            class="btn-primary account-menu-save"
            type="submit"
            :disabled="model.state.authSubmitting"
          >
            {{ submitLabelText }}
          </button>
          <button
            v-if="model.state.authDraft.auth_type === 'apikey' && showApiKeyEditor"
            class="btn-ghost account-menu-cancel"
            type="button"
            @click="cancelApiKeyEdit()"
          >
            Cancel
          </button>
          <button
            v-if="model.state.authDraft.auth_type !== 'apikey' && showUserpassEditor"
            class="btn-ghost account-menu-cancel"
            type="button"
            @click="cancelUserpassEdit()"
          >
            Cancel
          </button>
          <button class="btn-danger" type="button" @click="model.disconnectHydroServer()">
            Disconnect
          </button>
        </div>
      </form>
    </section>
  </div>
</template>
