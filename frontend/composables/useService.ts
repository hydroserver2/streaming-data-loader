import {
  getServiceStatus,
  installOsService,
  restartOsService,
  uninstallOsService,
  type ServiceStatusResponse,
} from "../api/os-service"
import { navigate } from "../router"
import { state } from "./state"

export function isServiceReady(status: ServiceStatusResponse | null | undefined): boolean {
  if (!status) return false
  if (!status.supported) return true
  return status.installed && status.running
}

export async function refreshServiceStatus(): Promise<ServiceStatusResponse | null> {
  state.serviceStatusLoading = true

  try {
    const status = await getServiceStatus()
    state.serviceStatus = status
    return status
  } catch (error) {
    state.serviceActionError =
      error instanceof Error
        ? error.message
        : "Couldn't determine the background service status."
    return null
  } finally {
    state.serviceStatusLoading = false
  }
}

async function runServiceAction(
  action: () => Promise<ServiceStatusResponse>
): Promise<void> {
  if (state.serviceActionSubmitting) return

  state.serviceActionSubmitting = true
  state.serviceActionError = null

  try {
    const status = await action()
    state.serviceStatus = status

    if (isServiceReady(status)) {
      const { bootstrap } = await import("./useAppModel")
      await bootstrap()
    } else {
      navigate("service")
    }
  } catch (error) {
    state.serviceActionError =
      error instanceof Error ? error.message : "The background service action failed."
  } finally {
    state.serviceActionSubmitting = false
  }
}

export async function installBackgroundService(): Promise<void> {
  await runServiceAction(() => installOsService())
}

export async function restartBackgroundService(): Promise<void> {
  await runServiceAction(() => restartOsService())
}

export async function uninstallBackgroundService(): Promise<void> {
  await runServiceAction(() => uninstallOsService())
}
