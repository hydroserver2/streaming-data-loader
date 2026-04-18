import test from "node:test";
import assert from "node:assert/strict";

import {
  applyConnectionValidationResult,
  createAuthFieldStates,
  credentialFields,
  resetAuthFieldStates,
  runAuthSubmission,
  validateAuthFieldsForSubmit,
  type AuthFieldName,
  type FieldValidationState,
} from "./auth-submit";
import type { ConnectionTestResponse, ServerConfig } from "./api/app";

function createServerConfig(
  overrides: Partial<ServerConfig> = {}
): ServerConfig {
  return {
    auth_type: "apikey",
    url: "https://example.com",
    api_key: "valid-key",
    username: "",
    password: "",
    workspace_id: "",
    workspace_name: "",
    ...overrides,
  };
}

function createMarker() {
  const fieldStates = createAuthFieldStates();

  const markField = (
    field: AuthFieldName,
    nextState: FieldValidationState["state"],
    message: string | null = null
  ) => {
    fieldStates[field] = { state: nextState, message };
  };

  return { fieldStates, markField };
}

function createResult(
  overrides: Partial<ConnectionTestResponse> = {}
): ConnectionTestResponse {
  return {
    ok: false,
    state: "error",
    message: "Generic auth error",
    invalid_field: null,
    instance_name: "example.com",
    workspace_id: null,
    workspace_name: null,
    workspace_count: 0,
    datastream_count: 0,
    permissions_ok: false,
    ...overrides,
  };
}

test("validateAuthFieldsForSubmit rejects malformed URLs", () => {
  const { fieldStates, markField } = createMarker();

  const valid = validateAuthFieldsForSubmit(
    createServerConfig({ url: "not-a-url" }),
    markField
  );

  assert.equal(valid, false);
  assert.deepEqual(fieldStates.url, {
    state: "invalid",
    message: "Please enter a full http:// or https:// URL.",
  });
});

test("validateAuthFieldsForSubmit requires an API key for API key auth", () => {
  const { fieldStates, markField } = createMarker();

  const valid = validateAuthFieldsForSubmit(
    createServerConfig({ api_key: "" }),
    markField
  );

  assert.equal(valid, false);
  assert.deepEqual(fieldStates.api_key, {
    state: "invalid",
    message: "Please enter your API key.",
  });
});

test("validateAuthFieldsForSubmit requires username, password, and workspace name for userpass auth", () => {
  const { fieldStates, markField } = createMarker();

  const valid = validateAuthFieldsForSubmit(
    createServerConfig({
      auth_type: "userpass",
      api_key: "",
      username: "",
      password: "",
    }),
    markField
  );

  assert.equal(valid, false);
  assert.deepEqual(fieldStates.username, {
    state: "invalid",
    message: "Please enter your username.",
  });
  assert.deepEqual(fieldStates.password, {
    state: "invalid",
    message: "Please enter your password.",
  });
  assert.deepEqual(fieldStates.workspace_name, {
    state: "invalid",
    message: "Please enter a workspace name.",
  });
});

test("applyConnectionValidationResult marks only the URL when HydroServer is unreachable", () => {
  const { fieldStates, markField } = createMarker();
  const server = createServerConfig();

  applyConnectionValidationResult(
    server,
    createResult({ message: "Couldn't reach HydroServer. Check the server URL and try again." }),
    markField
  );

  assert.equal(fieldStates.url.state, "invalid");
  assert.equal(fieldStates.api_key.state, "idle");
});

test("applyConnectionValidationResult marks the credential field when auth fails", () => {
  const { fieldStates, markField } = createMarker();
  const server = createServerConfig();

  applyConnectionValidationResult(
    server,
    createResult({ message: "That API key is invalid. Check the API key and try again." }),
    markField
  );

  assert.equal(fieldStates.url.state, "valid");
  assert.equal(fieldStates.api_key.state, "invalid");
  assert.equal(
    fieldStates.api_key.message,
    "That API key is invalid. Check the API key and try again."
  );
});

test("applyConnectionValidationResult marks only workspace name when the workspace is invalid", () => {
  const { fieldStates, markField } = createMarker();
  const server = createServerConfig({
    auth_type: "userpass",
    api_key: "",
    username: "user@example.com",
    password: "hunter2",
    workspace_name: "Missing Workspace",
  });

  applyConnectionValidationResult(
    server,
    createResult({
      invalid_field: "workspace_name",
      message:
        "No related workspace named \"Missing Workspace\" was found for this account. Check the workspace name and try again.",
    }),
    markField
  );

  assert.equal(fieldStates.url.state, "valid");
  assert.equal(fieldStates.username.state, "valid");
  assert.equal(fieldStates.password.state, "valid");
  assert.equal(fieldStates.workspace_name.state, "invalid");
  assert.equal(
    fieldStates.workspace_name.message,
    "No related workspace named \"Missing Workspace\" was found for this account. Check the workspace name and try again."
  );
});

test("resetAuthFieldStates clears state for the current auth mode", () => {
  const fieldStates = createAuthFieldStates();
  fieldStates.url = { state: "invalid", message: "Bad URL" };
  fieldStates.api_key = { state: "invalid", message: "Bad key" };
  fieldStates.username = { state: "invalid", message: "Bad user" };
  fieldStates.password = { state: "invalid", message: "Bad password" };
  fieldStates.workspace_name = { state: "invalid", message: "Bad workspace" };

  resetAuthFieldStates(fieldStates, "apikey");

  for (const field of ["url", ...credentialFields("apikey")] as AuthFieldName[]) {
    assert.deepEqual(fieldStates[field], { state: "idle", message: null });
  }
});

test("runAuthSubmission always clears submitting after success", async () => {
  const transitions: boolean[] = [];
  let renderCount = 0;

  const result = await runAuthSubmission({
    setSubmitting: (value) => {
      transitions.push(value);
    },
    render: () => {
      renderCount += 1;
    },
    action: async () => "done",
  });

  assert.equal(result, "done");
  assert.deepEqual(transitions, [true, false]);
  assert.equal(renderCount, 2);
});

test("runAuthSubmission always clears submitting after an early return path", async () => {
  const transitions: boolean[] = [];
  let renderCount = 0;

  await runAuthSubmission({
    setSubmitting: (value) => {
      transitions.push(value);
    },
    render: () => {
      renderCount += 1;
    },
    action: async () => {
      return;
    },
  });

  assert.deepEqual(transitions, [true, false]);
  assert.equal(renderCount, 2);
});

test("runAuthSubmission always clears submitting after an exception", async () => {
  const transitions: boolean[] = [];
  let renderCount = 0;

  await assert.rejects(() =>
    runAuthSubmission({
      setSubmitting: (value) => {
        transitions.push(value);
      },
      render: () => {
        renderCount += 1;
      },
      action: async () => {
        throw new Error("boom");
      },
    })
  );

  assert.deepEqual(transitions, [true, false]);
  assert.equal(renderCount, 2);
});
