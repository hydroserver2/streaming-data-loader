<script setup lang="ts">
import { computed } from 'vue'

import HeaderControls from '../components/HeaderControls.vue'
import { useAppModel } from '../composables/useAppModel'

const model = useAppModel()

const serviceStatus = computed(() => model.state.serviceStatus)
const isSupported = computed(() => serviceStatus.value?.supported !== false)
const isInstalled = computed(() => Boolean(serviceStatus.value?.installed))
const isRunning = computed(() => Boolean(serviceStatus.value?.running))
const isFirstInstall = computed(() => !isInstalled.value)
const actionLabel = computed(() => {
  if (model.state.serviceActionSubmitting) {
    if (isFirstInstall.value) return 'Installing Background Service...'
    if (isRunning.value) return 'Uninstalling Background Service...'
    return 'Restarting Background Service...'
  }

  if (isFirstInstall.value) return 'Install Background Service'
  if (isRunning.value) return 'Uninstall Background Service'
  return 'Restart Background Service'
})

async function handlePrimaryAction(): Promise<void> {
  if (!isSupported.value) return
  if (isInstalled.value && isRunning.value) {
    await model.uninstallBackgroundService()
    return
  }
  if (isInstalled.value && !isRunning.value) {
    await model.restartBackgroundService()
    return
  }

  if (!isInstalled.value) {
    await model.installBackgroundService()
  }
}
</script>

<template>
  <section class="page-shell animate-fade-in onboarding-shell service-shell">
    <header class="page-header wizard-header">
      <div class="wizard-header-bar">
        <div class="wizard-title-block">
          <h1 class="wizard-page-title">Streaming Data Loader Setup</h1>
          <!-- <p class="wizard-step-label">enable background service</p> -->
        </div>
        <div class="button-row wizard-actions">
          <HeaderControls />
        </div>
      </div>
    </header>

    <div class="service-layout">
      <section>
        <h2 class="service-subtitle">
          {{ isFirstInstall ? 'Enable background service' : 'Manage background service' }}
        </h2>
        <p v-if="isFirstInstall" class="service-copy">
          The Streaming Data Loader provides a lightweight operating system
          service that will detect changes in files you specify on your machine
          and push updates to HydroServer as soon as changes are made. Your data
          loading jobs keep working automatically, even after you close the app
          or log out of your user account. Because this service lives on the
          system level, your machine will ask for an administrator password to
          install it.
        </p>
        <template v-else>
          <p class="service-copy">
            You already have the Streaming Data Loader background service on
            this computer. Use this page if you want to remove it, or restart
            it if it has stopped running.
          </p>
          <p v-if="!isRunning" class="service-copy">
            The service appears to be installed but not currently running, so
            your background loading jobs will not continue until it is started
            again.
          </p>
        </template>
      </section>
      <section class="service-card service-card-muted">
        <p class="eyebrow">Note</p>
        <h2 class="service-subtitle">
          Uninstalling the app will not remove the background service.
        </h2>
        <p class="service-copy">
          Since the Streaming Data Loader app and background service are
          separate executables, you'll need to uninstall the background service
          from inside the app before you uninstall the app. Use the settings
          icon in the top-right corner whenever you want to uninstall it.
        </p>
      </section>

      <section class="service-card">
        <p v-if="model.state.serviceActionError" class="notice-error">
          {{ model.state.serviceActionError }}
        </p>

        <div class="service-actions">
          <button
            class="btn-ghost"
            type="button"
            :disabled="
              model.state.serviceStatusLoading ||
              model.state.serviceActionSubmitting
            "
            @click="model.refreshServiceStatus()"
          >
            Check Again
          </button>
          <button
            class="btn-primary service-primary-button"
            type="button"
            :disabled="!isSupported || model.state.serviceActionSubmitting"
            @click="handlePrimaryAction()"
          >
            <span
              v-if="model.state.serviceActionSubmitting"
              class="service-button-spinner"
              aria-hidden="true"
            />
            {{ actionLabel }}
          </button>
        </div>
      </section>
    </div>
  </section>
</template>
