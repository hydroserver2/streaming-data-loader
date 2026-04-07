<script setup lang="ts">
import { computed } from "vue"

import FeedbackBanner from "./FeedbackBanner.vue"
import { useAppModel, API_KEY_DOCS_URL } from "../composables/useAppModel"

const props = defineProps<{
  formId: "welcome-form" | "settings-form"
  submitLabel: string
}>()

const model = useAppModel()

const usingUserPass = computed(() => model.state.authDraft.auth_type === "userpass")
const submitLabelText = computed(() =>
  model.state.authSubmitting ? "Connecting..." : props.submitLabel
)
const feedback = computed(() =>
  props.formId === "welcome-form"
    ? model.state.welcomeFeedback
    : model.state.settingsFeedback
)

function fieldError(name: "url" | "api_key" | "username" | "password"): string | null {
  const fieldState = model.state.authFieldStates[name]
  return fieldState.state === "invalid" ? fieldState.message : null
}
</script>

<template>
  <form
    :id="formId"
    class="auth-card"
    autocomplete="off"
    @submit.prevent="model.submitAuthConfig(formId)"
  >
    <section class="card-section">
      <div class="auth-header">
        <slot name="icon" />
        <h1 class="page-title">Connect to HydroServer</h1>
      </div>

      <FeedbackBanner :feedback="feedback" />

      <label class="field">
        <span class="field-label-row">
          <span class="label">Host URL</span>
        </span>
        <input
          :value="model.state.authDraft.url"
          class="input"
          type="url"
          placeholder="https://playground.hydroserver.org"
          @input="model.updateAuthDraftField(formId, 'url', ($event.target as HTMLInputElement).value)"
        />
        <p v-if="fieldError('url')" class="field-error">{{ fieldError("url") }}</p>
      </label>

      <template v-if="usingUserPass">
        <label class="field">
          <span class="field-label-row">
            <span class="label">Username</span>
          </span>
          <input
            :value="model.state.authDraft.username"
            class="input"
            type="text"
            placeholder="name@example.com"
            @input="model.updateAuthDraftField(formId, 'username', ($event.target as HTMLInputElement).value)"
          />
          <p v-if="fieldError('username')" class="field-error">
            {{ fieldError("username") }}
          </p>
        </label>

        <label class="field">
          <span class="field-label-row">
            <span class="label">Password</span>
          </span>
          <input
            :value="model.state.authDraft.password"
            class="input"
            type="password"
            placeholder="Enter your HydroServer password"
            @input="model.updateAuthDraftField(formId, 'password', ($event.target as HTMLInputElement).value)"
          />
          <p v-if="fieldError('password')" class="field-error">
            {{ fieldError("password") }}
          </p>
        </label>
      </template>

      <template v-else>
        <label class="field">
          <span class="field-label-row">
            <span class="label">API key</span>
            <a
              class="label-link"
              :href="API_KEY_DOCS_URL"
              target="_blank"
              rel="noreferrer"
            >
              How to create an API key &rarr;
            </a>
          </span>
          <input
            :value="model.state.authDraft.api_key"
            class="input"
            type="password"
            placeholder="KaTz74swGqHn__I2VY6ceIzrIxC04oDhUrLLgBTH9ACxYIunmkrdmqk"
            @input="model.updateAuthDraftField(formId, 'api_key', ($event.target as HTMLInputElement).value)"
          />
          <p v-if="fieldError('api_key')" class="field-error">
            {{ fieldError("api_key") }}
          </p>
        </label>
      </template>

      <div class="auth-toggle-group">
        <span class="auth-divider-label">or</span>
        <button
          class="auth-toggle"
          type="button"
          @click="model.toggleAuthMode(formId)"
        >
          {{ usingUserPass ? "Connect with an API key" : "Connect with username and password" }}
        </button>
      </div>

      <div class="button-row button-row-end">
        <slot name="secondary" />
        <button
          class="btn-primary"
          type="submit"
          :disabled="model.state.authSubmitting"
        >
          {{ submitLabelText }}
        </button>
      </div>
    </section>
  </form>
</template>
