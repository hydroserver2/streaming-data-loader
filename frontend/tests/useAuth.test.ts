import test from "node:test"
import assert from "node:assert/strict"

import { emptyServerConfig, state } from "../composables/state"
import { submitAuthConfig, updateAuthDraftField } from "../composables/useAuth"
import { createAuthFieldStates } from "../auth-submit"

const originalFetch = globalThis.fetch
const originalWindow = globalThis.window

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  })
}

function resetAuthState(): void {
  state.authDraft = emptyServerConfig()
  state.authDraftDirty = false
  state.authFieldStates = createAuthFieldStates()
  state.authSubmitting = false
  state.connectionSummary = null
  state.lastConnectionState = null
  state.config = null
  state.serviceStatus = null
  state.serviceActionError = null
  state.jobStatuses = []
}

test.beforeEach(() => {
  resetAuthState()
  globalThis.fetch = originalFetch
  Object.defineProperty(globalThis, "window", {
    value: { location: { hash: "#welcome" } },
    configurable: true,
    writable: true,
  })
})

test.after(() => {
  globalThis.fetch = originalFetch
  if (originalWindow === undefined) {
    Reflect.deleteProperty(globalThis, "window")
  } else {
    Object.defineProperty(globalThis, "window", {
      value: originalWindow,
      configurable: true,
      writable: true,
    })
  }
})

test("submitAuthConfig saves a valid API key login state", async () => {
  const requests: string[] = []

  state.authDraft = {
    auth_type: "apikey",
    url: "https://example.com",
    api_key: "secret-key",
    username: "",
    password: "",
    workspace_id: "",
    workspace_name: "",
  }

  globalThis.fetch = async (input, init) => {
    const url = String(input)
    requests.push(`${init?.method ?? "GET"} ${url}`)

    if (url.includes("/connection/validate-url")) {
      return jsonResponse({
        ok: true,
        message: "Looks good.",
        instance_name: "Example",
      })
    }

    if (url.endsWith("/connection/test")) {
      return jsonResponse({
        ok: true,
        state: "connected",
        message: "Connected.",
        invalid_field: null,
        instance_name: "Example",
        workspace_id: "workspace-1",
        workspace_name: "Primary Workspace",
        workspace_count: 1,
        datastream_count: 12,
        permissions_ok: true,
      })
    }

    if (url.endsWith("/config/server")) {
      return jsonResponse({
        version: 1,
        server: {
          auth_type: "apikey",
          url: "https://example.com",
          api_key: "secret-key",
          username: "",
          password: "",
          workspace_id: "workspace-1",
          workspace_name: "Primary Workspace",
        },
        jobs: [],
      })
    }

    throw new Error(`Unexpected request: ${url}`)
  }

  await submitAuthConfig("welcome-form")

  assert.deepEqual(requests, [
    "GET /api/connection/validate-url?url=https%3A%2F%2Fexample.com",
    "POST /api/connection/test",
    "PUT /api/config/server",
    "POST /api/connection/test",
  ])
  assert.equal(state.authSubmitting, false)
  assert.equal(state.lastConnectionState, "connected")
  assert.equal(state.config?.server.workspace_id, "workspace-1")
  assert.equal(state.connectionSummary?.workspace_name, "Primary Workspace")
  assert.equal(state.authDraftDirty, false)
})

test("updating the host URL marks the auth draft dirty", () => {
  updateAuthDraftField("welcome-form", "url", "https://example.com")

  assert.equal(state.authDraft.url, "https://example.com")
  assert.equal(state.authDraftDirty, true)
})
