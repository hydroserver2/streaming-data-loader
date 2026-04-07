import type { ServerConfig, ConnectionTestResponse } from "./api";
import {
  getHealth,
  getConfig,
  listJobs,
  getDatastreams,
  getCsvPreview,
  testConnection,
  validateServerUrl,
  updateServerConfig,
  clearServerConfig,
  createJob,
  runJob,
  enableJob,
  disableJob,
  deleteJob,
} from "./api";
import {
  validateAuthFieldsForSubmit,
  applyConnectionValidationResult,
  runAuthSubmission,
  fieldFormFeedbackTarget,
} from "./auth-submit";
import { navigate } from "./router";
import { render } from "./render";
import {
  state,
  emptyServerConfig,
  applyPreview,
  resetPipelineState,
  readServerConfigForm,
  validatePipeline,
  setServerDraft,
  markField,
  resetStateAuthFieldStates,
  clearAuthValidationCache,
  serverConfigured,
  PREVIEW_PAGE_SIZE,
} from "./state";
import { basename, APP_NAME } from "./components/helpers";

const STARTUP_RETRY_ATTEMPTS = 12;
const STARTUP_RETRY_DELAY_MS = 350;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isTransientBootstrapError(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const msg = error.message.toLowerCase();
  return (
    msg.includes("failed to fetch") ||
    msg.includes("networkerror") ||
    msg.includes("status 500") ||
    msg.includes("status 502") ||
    msg.includes("status 503") ||
    msg.includes("status 504")
  );
}

async function loadInitialStateWithRetry(): Promise<{
  health: NonNullable<typeof state.health>;
  config: NonNullable<typeof state.config>;
  jobs: typeof state.jobs;
}> {
  let lastError: unknown = null;
  for (let attempt = 1; attempt <= STARTUP_RETRY_ATTEMPTS; attempt++) {
    try {
      const [health, config, jobs] = await Promise.all([
        getHealth(),
        getConfig(),
        listJobs(),
      ]);
      return { health, config, jobs };
    } catch (error) {
      lastError = error;
      if (attempt === STARTUP_RETRY_ATTEMPTS || !isTransientBootstrapError(error)) {
        throw error;
      }
      await sleep(STARTUP_RETRY_DELAY_MS);
    }
  }
  throw lastError instanceof Error
    ? lastError
    : new Error(`Failed to load ${APP_NAME}.`);
}

export async function syncAuthenticationStatus(
  server: ServerConfig
): Promise<ConnectionTestResponse> {
  const result = await testConnection(server);
  state.lastAuthValidationServer = server;
  state.lastAuthValidationResult = result;
  state.connectionSummary = result;
  state.lastConnectionState = result.state;
  if (result.ok && result.workspace_id) {
    if (state.config) state.config.server.workspace_id = result.workspace_id;
    state.authDraft.workspace_id = result.workspace_id;
  }
  if (!result.ok) {
    state.datastreams = [];
    state.datastreamsError = null;
  }
  return result;
}

export async function loadDatastreams(): Promise<void> {
  try {
    state.datastreams = await getDatastreams();
    state.datastreamsError = null;
  } catch (error) {
    state.datastreams = [];
    state.datastreamsError =
      error instanceof Error
        ? error.message
        : "Couldn't load HydroServer datastreams.";
  }
}

export async function bootstrap(): Promise<void> {
  state.loading = true;
  state.bootstrapError = null;
  state.welcomeFeedback = null;
  state.settingsFeedback = null;
  render();

  try {
    const { health, config, jobs } = await loadInitialStateWithRetry();
    state.health = health;
    state.config = config;
    state.authDraft = { ...emptyServerConfig(), ...config.server };
    state.jobs = jobs;
    state.lastConnectionState = health.connection.state;

    if (serverConfigured(config.server)) {
      const result = await syncAuthenticationStatus(config.server);
      if (result.ok) await loadDatastreams();
    }
  } catch (error) {
    state.bootstrapError =
      error instanceof Error ? error.message : `Failed to load ${APP_NAME}.`;
  } finally {
    state.loading = false;
    render();
  }
}

export async function refreshJobs(): Promise<void> {
  if (state.bootstrapError || state.loading) return;
  try {
    state.jobs = await listJobs();
    render();
  } catch {
    // Keep existing UI state on polling failure.
  }
}

export async function loadPipelinePreview(
  path: string,
  rows = PREVIEW_PAGE_SIZE
): Promise<void> {
  if (!path.trim()) {
    state.pipelineFeedback = {
      tone: "error",
      message: "Enter or choose a CSV file path first.",
    };
    render();
    return;
  }
  try {
    const preview = await getCsvPreview(path.trim(), rows);
    applyPreview(path.trim(), preview);
    state.pipelinePreviewRowsRequested = rows;
    state.pipelineErrors = [];
    state.pipelineFeedback = null;
  } catch (error) {
    state.pipelinePreview = null;
    state.pipelineSelectionTarget = null;
    state.pipelineDrag = null;
    state.pipelineColumnDrag = null;
    state.pipelinePreviewRowsRequested = PREVIEW_PAGE_SIZE;
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't preview that CSV file.",
    };
  }
  render();
}

export async function browseForCsvPath(): Promise<void> {
  try {
    const dialog = await import("@tauri-apps/plugin-dialog");
    const selection = await dialog.open({
      directory: false,
      multiple: false,
      filters: [{ name: "CSV files", extensions: ["csv", "txt"] }],
    });
    if (typeof selection !== "string" || !selection) return;
    state.pipelineForm.filePath = selection;
    if (!state.pipelineForm.name.trim()) {
      state.pipelineForm.name = basename(selection).replace(/\.[^.]+$/, "");
    }
    await loadPipelinePreview(selection);
  } catch {
    state.pipelineFeedback = {
      tone: "info",
      message:
        "The native file picker is only available in the desktop app. Enter the CSV path manually.",
    };
    render();
  }
}

export async function saveAuthenticatedServerConfig(
  form: HTMLFormElement
): Promise<void> {
  if (state.authSubmitting) return;

  const payload = readServerConfigForm(form);
  setServerDraft(payload);

  const feedbackKey = fieldFormFeedbackTarget(form.id) as
    | "welcomeFeedback"
    | "settingsFeedback";
  state[feedbackKey] = null;
  resetStateAuthFieldStates(payload.auth_type);

  if (!validateAuthFieldsForSubmit(payload, markField)) {
    render();
    return;
  }

  try {
    await runAuthSubmission({
      render,
      setSubmitting: (v) => {
        state.authSubmitting = v;
      },
      action: async () => {
        const urlValidation = await validateServerUrl(payload.url);
        if (!urlValidation.ok) {
          clearAuthValidationCache();
          markField("url", "invalid", urlValidation.message);
          state[feedbackKey] = { tone: "error", message: urlValidation.message };
          return;
        }
        markField("url", "valid");

        const result = await syncAuthenticationStatus(payload);
        applyConnectionValidationResult(payload, result, markField);
        if (!result.ok) {
          state[feedbackKey] = { tone: "error", message: result.message };
          return;
        }

        state.config = await updateServerConfig(payload);
        state.authDraft = { ...emptyServerConfig(), ...state.config.server };
        await syncAuthenticationStatus(state.config.server);
        await loadDatastreams();
        state[feedbackKey] = { tone: "success", message: result.message };
        state.settingsEditMode = false;
        navigate(state.jobs.length === 0 ? "jobs-new" : "dashboard");
      },
    });
  } catch (error) {
    clearAuthValidationCache();
    state[feedbackKey] = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't verify the HydroServer connection.",
    };
    state.lastConnectionState = "error";
    render();
  }
}

export async function disconnectHydroServer(): Promise<void> {
  try {
    state.config = await clearServerConfig();
    state.authDraft = emptyServerConfig();
    state.connectionSummary = null;
    state.lastConnectionState = "not_configured";
    state.datastreams = [];
    state.datastreamsError = null;
    state.welcomeFeedback = null;
    state.settingsFeedback = null;
    state.settingsEditMode = false;
    resetStateAuthFieldStates("apikey");
    clearAuthValidationCache();
    navigate("welcome");
  } catch (error) {
    state.settingsFeedback = {
      tone: "error",
      message:
        error instanceof Error
          ? error.message
          : "Couldn't disconnect from HydroServer right now.",
    };
  }
  render();
}

export async function savePipeline(): Promise<void> {
  state.pipelineErrors = validatePipeline();
  if (state.pipelineErrors.length > 0) {
    state.pipelineFeedback = {
      tone: "error",
      message: `${APP_NAME} needs a little more information before saving this pipeline.`,
    };
    render();
    return;
  }

  const mappedColumns = state.pipelineForm.mappings
    .filter((m) => m.datastreamId)
    .map((m) => {
      const ds = state.datastreams.find((d) => d.id === m.datastreamId);
      return {
        csv_column: m.csvColumn,
        datastream_id: m.datastreamId,
        datastream_name: ds?.name ?? m.datastreamId,
      };
    });

  try {
    const created = await createJob({
      name: state.pipelineForm.name.trim(),
      enabled: true,
      file_path: state.pipelineForm.filePath.trim(),
      schedule_minutes: state.pipelineForm.scheduleMinutes,
      file_config: {
        header_row: state.pipelineForm.hasHeaderRow ? state.pipelineForm.headerRow : 0,
        data_start_row: state.pipelineForm.dataStartRow,
        delimiter: state.pipelineForm.delimiter,
        timestamp_column: state.pipelineForm.timestampColumn,
        timestamp_format: state.pipelineForm.timestampFormat,
        timezone: state.pipelineForm.timezone,
      },
      column_mappings: mappedColumns,
    });

    state.jobs = [...state.jobs, created];
    resetPipelineState();
    navigate("dashboard");
  } catch (error) {
    state.pipelineFeedback = {
      tone: "error",
      message:
        error instanceof Error ? error.message : "Couldn't save that pipeline.",
    };
  }
  render();
}

export async function handleRunJob(jobId: string): Promise<void> {
  try {
    await runJob(jobId);
    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

export async function handleToggleJob(jobId: string): Promise<void> {
  const job = state.jobs.find((j) => j.id === jobId);
  if (!job) return;
  try {
    if (job.enabled) await disableJob(jobId);
    else await enableJob(jobId);
    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}

export async function handleDeleteJob(jobId: string): Promise<void> {
  if (!window.confirm("Delete this pipeline?")) return;
  try {
    await deleteJob(jobId);
    await refreshJobs();
  } catch {
    // Keep dashboard state unchanged on action failure.
  }
}
