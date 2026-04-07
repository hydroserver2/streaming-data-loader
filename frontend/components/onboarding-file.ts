import { state, previewHeaders } from "../state";
import { formatSchedule } from "../time";
import { escapeHtml, feedbackMarkup } from "./helpers";
import { renderPipelinePreview, previewFieldClass } from "./csv-preview";

const SCHEDULE_OPTIONS = [5, 15, 30, 60] as const;

function connectionBadge(): string {
  if (!state.connectionSummary?.instance_name) return "";
  return `
    <span class="onboarding-connection-badge">
      <span class="status-dot bg-emerald-500"></span>
      ${escapeHtml(state.connectionSummary.instance_name)}
    </span>
  `;
}

function renderFileSection(): string {
  return `
    <h2 class="section-title">Source file</h2>

    <label class="field">
      <span class="label">Pipeline name</span>
      <input
        class="input"
        type="text"
        name="pipeline_name"
        value="${escapeHtml(state.pipelineForm.name)}"
        placeholder="Little Bear River stage"
        autocomplete="off"
      />
    </label>

    <label class="field">
      <span class="label">CSV file path</span>
      <input
        class="input"
        type="text"
        name="file_path"
        value="${escapeHtml(state.pipelineForm.filePath)}"
        placeholder="/Users/you/datalogger/output.csv"
        autocomplete="off"
      />
      <span class="field-hint">
        This path is saved locally so new rows can be loaded in the background.
      </span>
    </label>

    <button class="btn-primary form-action" type="button" data-action="browse-csv">
      Choose CSV file
    </button>
  `;
}

function renderStructureSection(): string {
  const headers = previewHeaders();

  const timestampOptions = headers
    .map(
      (h) =>
        `<option value="${escapeHtml(h)}" ${
          h === state.pipelineForm.timestampColumn ? "selected" : ""
        }>${escapeHtml(h)}</option>`
    )
    .join("");

  const scheduleOptions = SCHEDULE_OPTIONS.map(
    (minutes) =>
      `<option value="${minutes}" ${
        state.pipelineForm.scheduleMinutes === minutes ? "selected" : ""
      }>Every ${formatSchedule(minutes).replace("Every ", "")}</option>`
  ).join("");

  return `
    <hr class="form-divider" />
    <h2 class="section-title">File structure</h2>

    <div class="split-fields">
      ${
        state.pipelineForm.hasHeaderRow
          ? `
            <label class="${previewFieldClass("header-row")}" for="pipeline-header-row">
              <span class="label">Header row</span>
              <input
                id="pipeline-header-row"
                class="input"
                type="number"
                min="1"
                name="header_row"
                value="${state.pipelineForm.headerRow}"
              />
              <span class="field-hint">
                Drag the blue HEADER handle or enter a row number.
              </span>
            </label>
          `
          : `
            <div class="field">
              <span class="label">Header row</span>
              <span class="field-hint">
                Using generated labels: Column 1, Column 2, Column 3…
              </span>
            </div>
          `
      }

      <label class="${previewFieldClass("data-start-row")}" for="pipeline-data-start-row">
        <span class="label">Data start row</span>
        <input
          id="pipeline-data-start-row"
          class="input"
          type="number"
          min="${state.pipelineForm.hasHeaderRow ? 2 : 1}"
          name="data_start_row"
          value="${state.pipelineForm.dataStartRow}"
        />
        <span class="field-hint">
          Drag the green DATA START handle or enter a row number.
        </span>
      </label>
    </div>

    <div class="split-fields">
      <label class="field">
        <span class="label">Delimiter</span>
        <input
          class="input"
          type="text"
          name="delimiter"
          value="${escapeHtml(state.pipelineForm.delimiter)}"
          maxlength="2"
        />
      </label>

      <label class="field">
        <span class="label">Timezone</span>
        <input
          class="input"
          type="text"
          name="timezone"
          value="${escapeHtml(state.pipelineForm.timezone)}"
        />
      </label>
    </div>

    <label class="${previewFieldClass("timestamp-column")}" for="pipeline-timestamp-column">
      <span class="label">Timestamp column</span>
      ${
        headers.length > 0
          ? `<select id="pipeline-timestamp-column" class="input" name="timestamp_column">
              ${timestampOptions}
            </select>`
          : `<input
              id="pipeline-timestamp-column"
              class="input"
              type="text"
              name="timestamp_column"
              value="${escapeHtml(state.pipelineForm.timestampColumn)}"
              placeholder="Timestamp"
            />`
      }
      <span class="field-hint">
        Drag the amber TIMESTAMP handle or click the matching column header.
      </span>
    </label>

    <label class="field">
      <span class="label">Timestamp format</span>
      <input
        class="input"
        type="text"
        name="timestamp_format"
        value="${escapeHtml(state.pipelineForm.timestampFormat)}"
        placeholder="%Y-%m-%d %H:%M:%S"
      />
    </label>

    <hr class="form-divider" />
    <h2 class="section-title">Schedule</h2>

    <label class="field">
      <span class="label">Check for new data</span>
      <select class="input" name="schedule_minutes">
        ${scheduleOptions}
      </select>
    </label>

    <button
      class="btn-primary form-action form-action-end"
      type="button"
      data-action="advance-to-mapping"
    >
      Next: Map columns &rarr;
    </button>
  `;
}

export function renderOnboardingFile(): string {
  const hasPreview = state.pipelinePreview !== null;
  const isFirstPipeline = state.jobs.length === 0;

  return `
    <section class="page-shell onboarding-shell animate-fade-in">
      <header class="onboarding-header">
        <p class="eyebrow">${isFirstPipeline ? "Step 1 of 2" : "New pipeline"}</p>
        <h1 class="page-title">Configure your data source</h1>
        <p class="page-copy">
          Choose a CSV file, inspect the preview, and mark the rows and column the loader should use.
        </p>
        ${connectionBadge()}
      </header>

      ${feedbackMarkup(state.pipelineFeedback)}

      <div class="pipeline-layout">
        <form id="pipeline-form" class="pipeline-form" autocomplete="off">
          ${renderFileSection()}
          ${hasPreview ? renderStructureSection() : ""}
        </form>

        ${renderPipelinePreview()}
      </div>
    </section>
  `;
}
