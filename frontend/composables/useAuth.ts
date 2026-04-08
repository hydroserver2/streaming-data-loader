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
import {
  createEmptyPipelineForm,
  emptyServerConfig,
  PREVIEW_PAGE_SIZE,
  state,
} from "./state"

export function serverConfigured(server: ServerConfig | null | undefined): boolean {
  if (!server?.url.trim()) return false
  if (server.auth_type === "userpass") {
    return Boolean(server.username.trim() && server.password.trim())
  }
  return Boolean(server.api_key.trim())
}

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

function normalizeServerDraft(): ServerConfig {
  const server = state.authDraft
  return {
    auth_type: server.auth_type,
    url: server.url.trim(),
    api_key: server.auth_type === "apikey" ? server.api_key.trim() : server.api_key,
    username: server.auth_type === "userpass" ? server.username.trim() : server.username,
    password: server.auth_type === "userpass" ? server.password.trim() : server.password,
    workspace_id: "",
  }
}

export function updateAuthDraftField(
  formId: "welcome-form" | "settings-form",
  field: AuthFieldName,
  value: string
): void {
  state.authDraft[field] = value
  clearFormFeedback(formId)
  markField(field, "idle")
}

export function toggleAuthMode(formId: "welcome-form" | "settings-form"): void {
  const nextType: AuthType =
    state.authDraft.auth_type === "apikey" ? "userpass" : "apikey"
  state.authDraft = { ...state.authDraft, auth_type: nextType }
  resetFieldStates(nextType)
  clearFormFeedback(formId)
}

export async function syncAuthenticationStatus(
  server: ServerConfig
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server)
  state.connectionSummary = result
  state.lastConnectionState = result.state

  if (result.ok && result.workspace_id) {
    if (state.config) state.config.server.workspace_id = result.workspace_id
    state.authDraft.workspace_id = result.workspace_id
  }

  return result
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
      setSubmitting: (value) => {
        state.authSubmitting = value
      },
      action: async () => {
        const urlValidation = await validateServerUrl(payload.url)
        if (!urlValidation.ok) {
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
        state[feedbackKey] = { tone: "success", message: result.message }
        navigate("jobs-new")
      },
    })
  } catch (error) {
    state[feedbackKey] = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't verify the HydroServer connection.",
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
    state.welcomeFeedback = null
    state.settingsFeedback = null
    state.pipelineForm = createEmptyPipelineForm()
    state.pipelinePreview = null
    state.pipelineSelectionTarget = null
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE
    state.pipelineValidationAttempted = false
    state.pipelineReadyForMapping = false
    state.validatedPipelineSettings = null
    state.pipelineDatastreams = []
    state.pipelineDatastreamsLoading = false
    state.pipelineMappingDrafts = []
    state.validatedColumnMappings = []
    resetFieldStates("apikey")
    navigate("welcome")
  } catch (error) {
    state.welcomeFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't disconnect from HydroServer right now.",
    }
  }
}
