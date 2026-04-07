<script setup lang="ts">
import { useAppModel } from "../composables/useAppModel"

const model = useAppModel()
</script>

<template>
  <section class="welcome-shell">
    <div class="welcome-card">
      <p class="eyebrow">Sidecar error</p>
      <h1 class="page-title">The background process is unavailable</h1>
      <p class="page-copy">
        {{ model.state.bootstrapError ?? `${model.APP_NAME} could not reach the local background service.` }}
      </p>
      <button class="btn-primary" type="button" @click="model.bootstrap()">
        Retry
      </button>
    </div>
  </section>
</template>
