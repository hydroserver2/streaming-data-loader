<script setup lang="ts">
import appIconUrl from "../../icons/icon-color.svg"

import AuthForm from "../components/AuthForm.vue"
import ConnectedCard from "../components/ConnectedCard.vue"
import FeedbackBanner from "../components/FeedbackBanner.vue"
import { APP_NAME, useAppModel } from "../composables/useAppModel"

const model = useAppModel()
const isConnected = model.isConnected
</script>

<template>
  <section class="page-shell animate-fade-in">
    <header class="page-header">
      <div>
        <p class="eyebrow">Settings</p>
        <h1 class="page-title">HydroServer connection</h1>
        <p class="page-copy">
          After {{ APP_NAME }} is connected, this form stays out of the way. Return here any
          time to rotate credentials or verify access again.
        </p>
      </div>
    </header>

    <FeedbackBanner :feedback="model.state.settingsFeedback" />

    <AuthForm
      v-if="!isConnected || model.state.settingsEditMode"
      form-id="settings-form"
      submit-label="Save and verify"
    >
      <template #icon>
        <img class="h-12 w-12" :src="appIconUrl" alt="HydroServer Streaming Data Loader icon" />
      </template>

      <template #secondary>
        <button
          v-if="isConnected"
          class="btn-ghost"
          type="button"
          @click="model.cancelCredentialEdit()"
        >
          Cancel
        </button>
      </template>
    </AuthForm>

    <ConnectedCard v-else :show-actions="true" />
  </section>
</template>
