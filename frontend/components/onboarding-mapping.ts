import { state } from "../state";
import { escapeHtml, feedbackMarkup } from "./helpers";

function connectionBadge(): string {
  if (!state.connectionSummary?.instance_name) return "";
  return `
    <span class="onboarding-connection-badge">
      <span class="status-dot bg-emerald-500"></span>
      ${escapeHtml(state.connectionSummary.instance_name)}
    </span>
  `;
}

function renderMappingRow(csvColumn: string, datastreamId: string): string {
  const isMapped = Boolean(datastreamId);

  const options = [
    `<option value="">Not mapped</option>`,
    ...state.datastreams.map(
      (ds) =>
        `<option value="${escapeHtml(ds.id)}" ${
          ds.id === datastreamId ? "selected" : ""
        }>${escapeHtml(ds.name)}</option>`
    ),
  ].join("");

  return `
    <div class="mapping-row ${isMapped ? "mapping-row-active" : ""}">
      <span class="mapping-source">${escapeHtml(csvColumn)}</span>
      <span class="mapping-connector" aria-hidden="true">
        <span class="mapping-connector-line ${isMapped ? "mapping-connector-line-active" : ""}"></span>
        <span class="mapping-connector-arrow">&#8594;</span>
      </span>
      <select
        class="input mapping-select"
        data-mapping-column="${escapeHtml(csvColumn)}"
      >${options}</select>
    </div>
  `;
}

function renderValidationErrors(): string {
  if (state.pipelineErrors.length === 0) return "";
  return `
    <div class="validation-panel">
      <h3 class="section-title">Fix these before saving</h3>
      <ul class="validation-list">
        ${state.pipelineErrors.map((e) => `<li>${escapeHtml(e)}</li>`).join("")}
      </ul>
    </div>
  `;
}

export function renderOnboardingMapping(): string {
  const { mappings } = state.pipelineForm;
  const isFirstPipeline = state.jobs.length === 0;
  const mappedCount = mappings.filter((m) => m.datastreamId).length;

  return `
    <section class="page-shell onboarding-shell animate-fade-in">
      <header class="onboarding-header">
        <div>
          <p class="eyebrow">${isFirstPipeline ? "Step 2 of 2" : "New pipeline"}</p>
          <h1 class="page-title">Map columns to datastreams</h1>
          <p class="page-copy">
            Connect each CSV source column to a HydroServer datastream.
            Leave unused columns as "Not mapped."
          </p>
        </div>
        ${connectionBadge()}
      </header>

      <div class="mapping-card">
        <div class="mapping-card-header">
          <span class="mapping-col-label">Source column</span>
          <span></span>
          <span class="mapping-col-label">HydroServer datastream</span>
        </div>

        ${
          mappings.length > 0
            ? `<div class="mapping-list">
                ${mappings
                  .map((m) => renderMappingRow(m.csvColumn, m.datastreamId))
                  .join("")}
              </div>`
            : `<p class="section-copy mapping-empty">
                No source columns found.
                <button class="btn-link" type="button" data-action="back-to-file-config">
                  Go back and load a CSV preview first.
                </button>
              </p>`
        }

        ${
          mappings.length > 0
            ? `<p class="mapping-summary">
                ${mappedCount} of ${mappings.length} column${mappings.length === 1 ? "" : "s"} mapped
              </p>`
            : ""
        }
      </div>

      ${renderValidationErrors()}
      ${feedbackMarkup(state.pipelineFeedback)}

      <div class="button-row">
        <button class="btn-ghost" type="button" data-action="back-to-file-config">
          &larr; Back
        </button>
        <button class="btn-primary" type="button" data-action="save-pipeline">
          Save pipeline
        </button>
      </div>
    </section>
  `;
}
