import "./generated.css";

import {
  state,
  emptyServerConfig,
  setServerDraft,
  readServerConfigForm,
  markField,
  resetStateAuthFieldStates,
  clearAuthValidationCache,
  clearAuthFormFeedback,
  updatePipelineField,
  setPipelineHasHeaderRow,
  applyPreviewLineSelection,
  applyPreviewColumnSelection,
  PREVIEW_PAGE_SIZE,
} from "./state";
import { initRenderer, render } from "./render";
import { initPreviewDragEvents, getSuppressHandleClick, clearSuppressHandleClick } from "./components/csv-preview";
import {
  bootstrap,
  refreshJobs,
  loadPipelinePreview,
  browseForCsvPath,
  saveAuthenticatedServerConfig,
  disconnectHydroServer,
  savePipeline,
  handleRunJob,
  handleToggleJob,
  handleDeleteJob,
} from "./actions";
import type { AuthType } from "./api";
import { navigate } from "./router";

// ── App shell elements ─────────────────────────────────────────────────────
const shellElements = {
  sidebar: document.querySelector<HTMLElement>("#app-sidebar"),
  mainContent: document.querySelector<HTMLElement>("#main-content"),
  jobsLink: document.querySelector<HTMLAnchorElement>('[data-route="dashboard"]'),
  settingsLink: document.querySelector<HTMLAnchorElement>('[data-route="settings"]'),
  connectionDot: document.querySelector<HTMLElement>("#connection-status-dot"),
};

if (
  !shellElements.sidebar ||
  !shellElements.mainContent ||
  !shellElements.jobsLink ||
  !shellElements.settingsLink ||
  !shellElements.connectionDot
) {
  throw new Error("App shell is missing required elements.");
}

const { sidebar, mainContent, jobsLink, settingsLink, connectionDot } = shellElements;

// ── Initialize renderer and drag events ───────────────────────────────────
initRenderer({ sidebar, mainContent, jobsLink, settingsLink, connectionDot });
initPreviewDragEvents(mainContent, render);

// ── Background polling ─────────────────────────────────────────────────────
window.setInterval(() => void refreshJobs(), 30_000);

// ── Route changes ──────────────────────────────────────────────────────────
window.addEventListener("hashchange", () => {
  state.settingsFeedback = null;
  render();
});

// ── Form submission ────────────────────────────────────────────────────────
mainContent.addEventListener("submit", (event) => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement)) return;
  event.preventDefault();

  if (form.id === "welcome-form" || form.id === "settings-form") {
    void saveAuthenticatedServerConfig(form);
  }
});

// ── Live input updates ─────────────────────────────────────────────────────
mainContent.addEventListener("input", (event) => {
  const target = event.target;
  if (
    !(
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    )
  ) {
    return;
  }

  // Auth forms: keep draft in sync and clear stale validation.
  if (target.form?.id === "welcome-form" || target.form?.id === "settings-form") {
    const form = target.form;
    setServerDraft(readServerConfigForm(form));
    clearAuthFormFeedback(form.id);
    clearAuthValidationCache();
    if (
      target instanceof HTMLInputElement &&
      (target.name === "url" ||
        target.name === "api_key" ||
        target.name === "username" ||
        target.name === "password")
    ) {
      markField(target.name as "url" | "api_key" | "username" | "password", "idle");
    }
    return;
  }

  if (target.form?.id !== "pipeline-form") return;

  state.pipelineFeedback = null;
  state.pipelineErrors = [];

  // Mapping dropdown: update the specific mapping entry.
  const mappingColumn = (target as HTMLElement).dataset.mappingColumn;
  if (mappingColumn) {
    const mapping = state.pipelineForm.mappings.find(
      (m) => m.csvColumn === mappingColumn
    );
    if (mapping) mapping.datastreamId = target.value;
    render();
    return;
  }

  // Pipeline form fields: update state, re-render for structural changes.
  updatePipelineField(target.name, target.value);
  if (
    target.name === "header_row" ||
    target.name === "data_start_row" ||
    target.name === "delimiter" ||
    target.name === "timestamp_column"
  ) {
    render();
  }
});

// ── Change events ──────────────────────────────────────────────────────────
mainContent.addEventListener("change", (event) => {
  const target = event.target;
  if (
    !(
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    )
  ) {
    return;
  }

  if (target.name === "has_header_row" && target instanceof HTMLInputElement) {
    setPipelineHasHeaderRow(target.checked);
    render();
    return;
  }

  if (target.form?.id === "pipeline-form" && target.name === "file_path") {
    void loadPipelinePreview(target.value);
  }
});

// ── Click delegation ───────────────────────────────────────────────────────
mainContent.addEventListener("click", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;

  const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
  const jobId = target.closest<HTMLElement>("[data-job-id]")?.dataset.jobId;

  if (!action) return;

  switch (action) {
    // ── Bootstrap ─────────────────────────────────────────────────────────
    case "retry-bootstrap":
      void bootstrap();
      break;

    // ── Auth ───────────────────────────────────────────────────────────────
    case "toggle-auth-mode": {
      const form = target.closest<HTMLFormElement>("form");
      if (!form) break;
      const current = readServerConfigForm(form);
      const nextAuthType: AuthType =
        current.auth_type === "apikey" ? "userpass" : "apikey";
      setServerDraft({ ...current, auth_type: nextAuthType });
      resetStateAuthFieldStates(nextAuthType);
      clearAuthFormFeedback(form.id);
      clearAuthValidationCache();
      render();
      break;
    }

    case "disconnect":
      void disconnectHydroServer();
      break;

    case "change-credentials":
      state.authDraft = { ...emptyServerConfig(), ...(state.config?.server ?? {}) };
      state.settingsEditMode = true;
      navigate("settings");
      render();
      break;

    case "cancel-credential-edit":
      state.authDraft = { ...emptyServerConfig(), ...(state.config?.server ?? {}) };
      state.settingsEditMode = false;
      render();
      break;

    // ── Pipeline wizard ────────────────────────────────────────────────────
    case "browse-csv":
      void browseForCsvPath();
      break;

    case "advance-to-mapping":
      state.onboardingStep = "column-mapping";
      state.pipelineErrors = [];
      render();
      break;

    case "back-to-file-config":
      state.onboardingStep = "file-config";
      state.pipelineErrors = [];
      render();
      break;

    case "save-pipeline":
      void savePipeline();
      break;

    // ── Preview pagination ─────────────────────────────────────────────────
    case "show-more-preview-lines":
      if (state.pipelinePreview) {
        const nextRows = Math.min(
          state.pipelinePreviewRowsRequested + PREVIEW_PAGE_SIZE,
          state.pipelinePreview.total_lines
        );
        void loadPipelinePreview(state.pipelineForm.filePath, nextRows);
      }
      break;

    // ── Preview handle click (fires after a pointer-up, may need suppression)
    case "activate-preview-handle": {
      if (getSuppressHandleClick()) {
        clearSuppressHandleClick();
        break;
      }
      const pickerTarget = target.closest<HTMLElement>(
        "[data-preview-handle-target]"
      )?.dataset.previewHandleTarget;
      if (pickerTarget === "header-row" || pickerTarget === "data-start-row") {
        state.pipelineSelectionTarget = pickerTarget;
        render();
      }
      break;
    }

    // ── Preview row / column selection via click ───────────────────────────
    case "pick-preview-line": {
      const lineNumber = Number(
        target.closest<HTMLElement>("[data-preview-line]")?.dataset.previewLine
      );
      if (Number.isFinite(lineNumber)) {
        applyPreviewLineSelection(lineNumber);
        render();
      }
      break;
    }

    case "pick-preview-column": {
      const columnName =
        target.closest<HTMLElement>("[data-preview-column]")?.dataset
          .previewColumn ?? "";
      if (columnName) {
        applyPreviewColumnSelection(columnName);
        render();
      }
      break;
    }

    // ── Dashboard job actions ──────────────────────────────────────────────
    default:
      if (!jobId) break;
      if (action === "run-job") void handleRunJob(jobId);
      else if (action === "toggle-job") void handleToggleJob(jobId);
      else if (action === "delete-job") void handleDeleteJob(jobId);
  }
});

// ── Start ──────────────────────────────────────────────────────────────────
void bootstrap();
