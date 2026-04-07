import {
  applyConnectionValidationResult,
  fieldFormFeedbackTarget,
  resetAuthFieldStates,
  runAuthSubmission,
  validateAuthFieldsForSubmit,
  type AuthFieldName,
  type FieldValidationState,
} from "../auth-submit"
import {
  clearServerConfig,
  testConnection,
  updateServerConfig,
  validateServerUrl,
  type AuthType,
  type ConnectionTestResponse,
  type ServerConfig,
} from "../api"
import { navigate } from "../router"
import { state, emptyServerConfig } from "./state"
import { getDatastreams } from "../api"

// ── Server config helpers ──────────────────────────────────────────────────
export function serverConfigured(server: ServerConfig | null | undefined): boolean {
  if (!server?.url.trim()) return false
  if (server.auth_type === "userpass") {
    return Boolean(server.username.trim() && server.password.trim())
  }
  return Boolean(server.api_key.trim())
}

// ── Auth field helpers ─────────────────────────────────────────────────────
export function markField(
  field: AuthFieldName,
  nextState: FieldValidationState["state"],
  message: string | null = null
): void {
  state.authFieldStates[field] = { state: nextState, message }
}

function resetFieldStates(authType: AuthType): void {
  resetAuthFieldStates(state.authFieldStates, authType)
}

function clearFormFeedback(formId: "welcome-form" | "settings-form"): void {
  state[fieldFormFeedbackTarget(formId)] = null
}

function clearValidationCache(): void {
  state.lastAuthValidationServer = null
  state.lastAuthValidationResult = null
}

function normalizeServerDraft(): ServerConfig {
  const s = state.authDraft
  return {
    auth_type: s.auth_type,
    url: s.url.trim(),
    api_key: s.auth_type === "apikey" ? s.api_key.trim() : s.api_key,
    username: s.auth_type === "userpass" ? s.username.trim() : s.username,
    password: s.auth_type === "userpass" ? s.password.trim() : s.password,
    workspace_id: "",
  }
}

// ── Exported auth actions ─────────────────────────────────────────────────
export function updateAuthDraftField(
  formId: "welcome-form" | "settings-form",
  field: AuthFieldName,
  value: string
): void {
  state.authDraft[field] = value
  clearFormFeedback(formId)
  clearValidationCache()
  markField(field, "idle")
}

export function toggleAuthMode(formId: "welcome-form" | "settings-form"): void {
  const next: AuthType = state.authDraft.auth_type === "apikey" ? "userpass" : "apikey"
  state.authDraft = { ...state.authDraft, auth_type: next }
  resetFieldStates(next)
  clearFormFeedback(formId)
  clearValidationCache()
}

export async function syncAuthenticationStatus(
  server: ServerConfig
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server)
  state.lastAuthValidationServer = server
  state.lastAuthValidationResult = result
  state.connectionSummary = result
  state.lastConnectionState = result.state

  if (result.ok && result.workspace_id) {
    if (state.config) state.config.server.workspace_id = result.workspace_id
    state.authDraft.workspace_id = result.workspace_id
  }

  if (!result.ok) {
    state.datastreams = []
    state.datastreamsError = null
  }

  return result
}

export async function loadDatastreams(): Promise<void> {
  try {
    state.datastreams = await getDatastreams()
    state.datastreamsError = null
  } catch (error) {
    state.datastreams = []
    state.datastreamsError =
      error instanceof Error ? error.message : "Couldn't load HydroServer datastreams."
  }
}

export async function submitAuthConfig(
  formId: "welcome-form" | "settings-form"
): Promise<void> {
  if (state.authSubmitting) return

  const payload = normalizeServerDraft()
  state.authDraft = { ...payload }

  const feedbackKey = fieldFormFeedbackTarget(formId)
  state[feedbackKey] = null
  resetFieldStates(payload.auth_type)

  if (!validateAuthFieldsForSubmit(payload, markField)) return

  try {
    await runAuthSubmission({
      render: () => undefined,
      setSubmitting: (value) => { state.authSubmitting = value },
      action: async () => {
        const urlValidation = await validateServerUrl(payload.url)
        if (!urlValidation.ok) {
          clearValidationCache()
          markField("url", "invalid", urlValidation.message)
          state[feedbackKey] = { tone: "error", message: urlValidation.message }
          return
        }

        markField("url", "valid")

        const result = await syncAuthenticationStatus(payload)
        applyConnectionValidationResult(payload, result, markField)
        if (!result.ok) {
          state[feedbackKey] = { tone: "error", message: result.message }
          return
        }

        state.config = await updateServerConfig(payload)
        state.authDraft = { ...emptyServerConfig(), ...state.config.server }
        await syncAuthenticationStatus(state.config.server)
        await loadDatastreams()
        state[feedbackKey] = { tone: "success", message: result.message }
        state.settingsEditMode = false
        navigate(state.jobs.length === 0 ? "jobs-new" : "dashboard")
      },
    })
  } catch (error) {
    clearValidationCache()
    state[feedbackKey] = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't verify the HydroServer connection.",
    }
    state.lastConnectionState = "error"
  }
}

export async function disconnectHydroServer(): Promise<void> {
  try {
    state.config = await clearServerConfig()
    state.authDraft = emptyServerConfig()
    state.connectionSummary = null
    state.lastConnectionState = "not_configured"
    state.datastreams = []
    state.datastreamsError = null
    state.welcomeFeedback = null
    state.settingsFeedback = null
    state.settingsEditMode = false
    resetFieldStates("apikey")
    clearValidationCache()
    navigate("welcome")
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message: error instanceof Error ? error.message : "Couldn't disconnect from HydroServer right now.",
    }
  }
}

export function changeCredentials(): void {
  state.authDraft = { ...emptyServerConfig(), ...(state.config?.server ?? {}) }
  state.settingsEditMode = true
  navigate("settings")
}

export function cancelCredentialEdit(): void {
  state.authDraft = { ...emptyServerConfig(), ...(state.config?.server ?? {}) }
  state.settingsEditMode = false
}
