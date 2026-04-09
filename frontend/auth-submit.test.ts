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
import type { ConnectionTestResponse, ServerConfig } from "./api";

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
    message: "Enter a full http:// or https:// URL.",
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
    message: "Enter the API key.",
  });
});

test("validateAuthFieldsForSubmit requires username and password for userpass auth", () => {
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
    message: "Enter the username.",
  });
  assert.deepEqual(fieldStates.password, {
    state: "invalid",
    message: "Enter the password.",
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

test("resetAuthFieldStates clears state for the current auth mode", () => {
  const fieldStates = createAuthFieldStates();
  fieldStates.url = { state: "invalid", message: "Bad URL" };
  fieldStates.api_key = { state: "invalid", message: "Bad key" };
  fieldStates.username = { state: "invalid", message: "Bad user" };
  fieldStates.password = { state: "invalid", message: "Bad password" };

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
