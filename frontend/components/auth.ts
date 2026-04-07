import appIconUrl from "../../icons/icon-color.svg";
import { state, connected, serverConfigured } from "../state";
import { routeHref } from "../router";
import { escapeHtml, feedbackMarkup, APP_NAME } from "./helpers";
import type { AuthFieldName } from "../auth-submit";

const API_KEY_DOCS_URL =
  "https://hydroserver2.github.io/hydroserver/tutorials/creating-your-first-orchestration-system#create-an-api-key";

// ── Connection indicator ───────────────────────────────────────────────────
export function connectionIndicator(): { label: string; className: string } {
  if (!serverConfigured(state.config?.server)) {
    return {
      label: "HydroServer not configured",
      className: "status-dot bg-slate-300",
    };
  }
  if (connected()) {
    return {
      label: "Connected to HydroServer",
      className: "status-dot bg-emerald-500",
    };
  }
  if (state.lastConnectionState === "error") {
    return {
      label: "HydroServer authentication error",
      className: "status-dot bg-rose-500",
    };
  }
  return {
    label: "HydroServer configured",
    className: "status-dot bg-sky-500",
  };
}

// ── Auth form field ────────────────────────────────────────────────────────
function authFieldErrorMarkup(field: AuthFieldName): string {
  const fieldState = state.authFieldStates[field];
  if (fieldState.state !== "invalid" || !fieldState.message) return "";
  return `<p class="field-error">${escapeHtml(fieldState.message)}</p>`;
}

function renderAuthInputField(params: {
  label: string;
  name: AuthFieldName;
  type: "url" | "text" | "password";
  value: string;
  placeholder: string;
  helpText?: string;
  labelAction?: string;
}): string {
  const { label, name, type, value, placeholder, helpText, labelAction } = params;
  return `
    <label class="field">
      <span class="field-label-row">
        <span class="label">${escapeHtml(label)}</span>
        ${labelAction ?? ""}
      </span>
      <input
        class="input"
        type="${type}"
        name="${name}"
        value="${escapeHtml(value)}"
        placeholder="${escapeHtml(placeholder)}"
      />
      ${helpText ? `<p class="field-hint">${escapeHtml(helpText)}</p>` : ""}
      ${authFieldErrorMarkup(name)}
    </label>
  `;
}

// ── Auth form ──────────────────────────────────────────────────────────────
export function renderAuthForm(
  formId: "welcome-form" | "settings-form",
  submitLabel: string,
  secondaryAction: string
): string {
  const server = state.authDraft;
  const usingUserPass = server.auth_type === "userpass";
  const authToggleLabel = usingUserPass
    ? "Connect with an API key"
    : "Connect with username and password";
  const submitDisabled = state.authSubmitting ? "disabled" : "";
  const submitLabelText = state.authSubmitting ? "Connecting…" : submitLabel;

  return `
    <form id="${formId}" class="auth-card" autocomplete="off">
      <section class="card-section">
        <div class="auth-header">
          <img
            class="auth-app-icon"
            src="${appIconUrl}"
            alt="${APP_NAME} icon"
          />
          <h1 class="page-title">Connect to HydroServer</h1>
        </div>

        <input type="hidden" name="auth_type" value="${server.auth_type}" />

        ${renderAuthInputField({
          label: "Host URL",
          name: "url",
          type: "url",
          value: server.url,
          placeholder: "https://playground.hydroserver.org",
        })}

        ${
          usingUserPass
            ? `
              ${renderAuthInputField({
                label: "Username",
                name: "username",
                type: "text",
                value: server.username,
                placeholder: "name@example.com",
              })}
              ${renderAuthInputField({
                label: "Password",
                name: "password",
                type: "password",
                value: server.password,
                placeholder: "Enter your HydroServer password",
              })}
            `
            : `
              ${renderAuthInputField({
                label: "API key",
                name: "api_key",
                type: "password",
                value: server.api_key,
                placeholder:
                  "KaTz74swGqHn__I2VY6ceIzrIxC04oDhUrLLgBTH9ACxYIunmkrdmqk",
                labelAction: `<a class="label-link" href="${API_KEY_DOCS_URL}" target="_blank" rel="noreferrer">How to create an API key &rarr;</a>`,
              })}
            `
        }

        <div class="auth-toggle-group">
          <span class="auth-divider-label">or</span>
          <button class="auth-toggle" type="button" data-action="toggle-auth-mode">
            ${escapeHtml(authToggleLabel)}
          </button>
        </div>

        <div class="button-row button-row-end">
          ${secondaryAction}
          <button class="btn-primary" type="submit" ${submitDisabled}>
            ${escapeHtml(submitLabelText)}
          </button>
        </div>
      </section>
    </form>
  `;
}

// ── Pages ──────────────────────────────────────────────────────────────────
export function renderWelcome(): string {
  return `
    <section class="welcome-shell">
      ${renderAuthForm("welcome-form", "Connect to HydroServer", "")}
    </section>
  `;
}

export function renderSettings(): string {
  const showForm = !connected() || state.settingsEditMode;

  return `
    <section class="page-shell animate-fade-in">
      <header class="page-header">
        <div>
          <p class="eyebrow">Settings</p>
          <h1 class="page-title">HydroServer connection</h1>
          <p class="page-copy">
            After ${APP_NAME} is connected, this form stays out of the way.
            Return here any time to rotate credentials or verify access.
          </p>
        </div>
      </header>

      ${feedbackMarkup(state.settingsFeedback)}

      ${
        showForm
          ? renderAuthForm(
              "settings-form",
              "Save and verify",
              connected()
                ? `<button class="btn-ghost" type="button" data-action="cancel-credential-edit">Cancel</button>`
                : ""
            )
          : renderConnectedCard(true)
      }
    </section>
  `;
}

export function renderConnectedCard(showActions: boolean): string {
  if (!connected() || !state.connectionSummary) return "";

  const datastreamText =
    state.connectionSummary.datastream_count === 1
      ? "1 datastream available"
      : `${state.connectionSummary.datastream_count} datastreams available`;

  return `
    <article class="summary-card">
      <div class="summary-card-copy">
        <p class="eyebrow">Authenticated</p>
        <h2 class="section-title">
          ${escapeHtml(state.connectionSummary.instance_name ?? "HydroServer")}
        </h2>
        <p class="section-copy">${escapeHtml(state.connectionSummary.message)}</p>
        <div class="summary-inline">
          <span class="pill-success">Connected</span>
          <span class="summary-meta">${escapeHtml(datastreamText)}</span>
        </div>
      </div>
      ${
        showActions
          ? `
        <div class="button-row">
          <button class="btn-danger" type="button" data-action="disconnect">
            Disconnect
          </button>
          <button class="btn-ghost" type="button" data-action="change-credentials">
            Change credentials
          </button>
          ${
            state.jobs.length === 0
              ? `<a class="btn-primary" href="${routeHref("jobs-new")}">Create first pipeline</a>`
              : ""
          }
        </div>
      `
          : ""
      }
    </article>
  `;
}

export function renderFatalError(): string {
  return `
    <section class="welcome-shell">
      <div class="welcome-card">
        <p class="eyebrow">Sidecar error</p>
        <h1 class="page-title">The background process is unavailable</h1>
        <p class="page-copy">
          ${escapeHtml(
            state.bootstrapError ??
              `${APP_NAME} could not reach the local background service.`
          )}
        </p>
        <button class="btn-primary" type="button" data-action="retry-bootstrap">
          Retry
        </button>
      </div>
    </section>
  `;
}

export function renderLoading(): string {
  return `
    <section class="loading-shell" aria-label="Loading">
      <div class="loading-spinner" aria-hidden="true"></div>
    </section>
  `;
}
