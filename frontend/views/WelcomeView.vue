<script setup lang="ts">
import appIconUrl from "../../icons/icon-color-thick.svg"

import AuthForm from "../components/AuthForm.vue"
</script>

<template>
  <section class="welcome-shell">
    <AuthForm form-id="welcome-form" submit-label="Connect to HydroServer">
      <template #icon>
        <img class="auth-app-icon" :src="appIconUrl" alt="HydroServer Streaming Data Loader icon" />
      </template>
    </AuthForm>
  </section>
</template>
