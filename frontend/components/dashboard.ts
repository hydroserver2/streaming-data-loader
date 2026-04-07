import { state } from "../state";
import { routeHref } from "../router";
import { formatRelativeTime, formatSchedule, shortenPath } from "../time";
import { escapeHtml } from "./helpers";
import type { JobSummary } from "../api";

function statusPill(job: JobSummary): string {
  const classes: Record<JobSummary["status"], string> = {
    healthy: "pill-success",
    warning: "pill-warning",
    error: "pill-danger",
    disabled: "pill-muted",
    pending: "pill-info",
    running: "pill-info",
  };
  return `<span class="${classes[job.status]}">${escapeHtml(job.status_message)}</span>`;
}

function jobStatusDotClass(status: JobSummary["status"]): string {
  switch (status) {
    case "error":
      return "bg-rose-500";
    case "warning":
      return "bg-amber-500";
    case "disabled":
      return "bg-slate-300";
    default:
      return "bg-emerald-500";
  }
}

function renderJobCard(job: JobSummary): string {
  const lastLine = job.last_error
    ? `Failed ${formatRelativeTime(job.last_run_at)}`
    : `Last pushed ${formatRelativeTime(job.last_pushed_timestamp)}`;

  return `
    <article class="job-card animate-fade-in">
      <div class="job-card-top">
        <div>
          <div class="job-card-title-row">
            <span class="status-dot ${jobStatusDotClass(job.status)}"></span>
            <h2 class="section-title">${escapeHtml(job.name)}</h2>
          </div>
          <p class="section-copy">${escapeHtml(shortenPath(job.file_path))}</p>
          <p class="job-meta ${job.status === "error" ? "text-rose-600" : ""}">
            ${escapeHtml(lastLine)} · ${escapeHtml(formatSchedule(job.schedule_minutes))}
          </p>
        </div>
        ${statusPill(job)}
      </div>

      <div class="job-card-actions">
        <button class="btn-ghost" data-action="run-job" data-job-id="${job.id}">
          Run now
        </button>
        <button class="btn-ghost" data-action="toggle-job" data-job-id="${job.id}">
          ${job.enabled ? "Disable" : "Enable"}
        </button>
        <button class="btn-danger" data-action="delete-job" data-job-id="${job.id}">
          Delete
        </button>
      </div>
    </article>
  `;
}

export function renderDashboard(): string {
  if (state.jobs.length === 0) {
    return `
      <section class="page-shell animate-fade-in">
        <header class="page-header">
          <div>
            <p class="eyebrow">Dashboard</p>
            <h1 class="page-title">No pipelines yet</h1>
            <p class="page-copy">
              Create your first pipeline to start streaming CSV data into HydroServer.
            </p>
          </div>
          <a class="btn-primary" href="${routeHref("jobs-new")}">Create first pipeline</a>
        </header>
      </section>
    `;
  }

  return `
    <section class="page-shell">
      <header class="page-header">
        <div>
          <p class="eyebrow">Dashboard</p>
          <h1 class="page-title">Pipelines</h1>
          <p class="page-copy">
            Your pipelines watch local CSV sources, track row cursors, and push
            only new observations into HydroServer.
          </p>
        </div>
        <a class="btn-primary" href="${routeHref("jobs-new")}">Add pipeline</a>
      </header>
      <div class="card-stack">
        ${state.jobs.map(renderJobCard).join("")}
      </div>
    </section>
  `;
}
