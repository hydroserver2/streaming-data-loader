import { invokeCommand, isTauriRuntime } from "../runtime"
import type { ServiceStatusResponse } from "./types"

export function getServiceStatus(): Promise<ServiceStatusResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ServiceStatusResponse>("get_service_status")
  }

  return Promise.reject(
    new Error("Background service management is only available in the desktop app.")
  )
}

export function installOsService(): Promise<ServiceStatusResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ServiceStatusResponse>("install_os_service")
  }

  return Promise.reject(
    new Error("Background service management is only available in the desktop app.")
  )
}

export function restartOsService(): Promise<ServiceStatusResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ServiceStatusResponse>("restart_os_service")
  }

  return Promise.reject(
    new Error("Background service management is only available in the desktop app.")
  )
}

export function uninstallOsService(): Promise<ServiceStatusResponse> {
  if (isTauriRuntime()) {
    return invokeCommand<ServiceStatusResponse>("uninstall_os_service")
  }

  return Promise.reject(
    new Error("Background service management is only available in the desktop app.")
  )
}
