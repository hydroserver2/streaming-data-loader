import type {
  AuthType,
  ConnectionTestResponse,
  ServerConfig,
} from "./api";

export type AuthFieldName =
  | "url"
  | "api_key"
  | "username"
  | "password"
  | "workspace_name";

export type FieldValidationState = {
  state: "idle" | "checking" | "valid" | "invalid";
  message: string | null;
};

export type AuthFieldStates = Record<AuthFieldName, FieldValidationState>;

export function emptyFieldValidationState(): FieldValidationState {
  return { state: "idle", message: null };
}

export function createAuthFieldStates(): AuthFieldStates {
  return {
    url: emptyFieldValidationState(),
    api_key: emptyFieldValidationState(),
    username: emptyFieldValidationState(),
    password: emptyFieldValidationState(),
    workspace_name: emptyFieldValidationState(),
  };
}

export function resetAuthFieldStates(
  authFieldStates: AuthFieldStates,
  authType: AuthType
): void {
  authFieldStates.url = emptyFieldValidationState();
  authFieldStates.api_key = emptyFieldValidationState();
  authFieldStates.username = emptyFieldValidationState();
  authFieldStates.password = emptyFieldValidationState();
  authFieldStates.workspace_name = emptyFieldValidationState();

  if (authType === "apikey") {
    authFieldStates.username = emptyFieldValidationState();
    authFieldStates.password = emptyFieldValidationState();
    authFieldStates.workspace_name = emptyFieldValidationState();
  } else {
    authFieldStates.api_key = emptyFieldValidationState();
  }
}

export function credentialFields(authType: AuthType): AuthFieldName[] {
  return authType === "userpass"
    ? ["username", "password", "workspace_name"]
    : ["api_key"];
}

export function isValidHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function validateAuthFieldsForSubmit(
  server: ServerConfig,
  markField: (
    field: AuthFieldName,
    nextState: FieldValidationState["state"],
    message?: string | null
  ) => void
): boolean {
  let valid = true;

  if (!server.url) {
    markField("url", "invalid", "Please enter your HydroServer URL.");
    valid = false;
  } else if (!isValidHttpUrl(server.url)) {
    markField("url", "invalid", "Please enter a full http:// or https:// URL.");
    valid = false;
  } else {
    markField("url", "valid");
  }

  if (server.auth_type === "apikey") {
    if (!server.api_key) {
      markField("api_key", "invalid", "Please enter your API key.");
      valid = false;
    } else {
      markField("api_key", "valid");
    }
  } else {
    if (!server.username) {
      markField("username", "invalid", "Please enter your username.");
      valid = false;
    } else {
      markField("username", "valid");
    }

    if (!server.password) {
      markField("password", "invalid", "Please enter your password.");
      valid = false;
    } else {
      markField("password", "valid");
    }

    if (!server.workspace_name.trim()) {
      markField("workspace_name", "invalid", "Please enter a workspace name.");
      valid = false;
    } else {
      markField("workspace_name", "valid");
    }
  }

  return valid;
}

export function applyConnectionValidationResult(
  server: ServerConfig,
  result: ConnectionTestResponse,
  markField: (
    field: AuthFieldName,
    nextState: FieldValidationState["state"],
    message?: string | null
  ) => void
): void {
  markField("url", "valid");

  if (result.ok) {
    for (const field of credentialFields(server.auth_type)) {
      markField(field, "valid");
    }
    return;
  }

  const message = result.message;
  const isUrlError =
    result.message.includes("Couldn't reach HydroServer") ||
    result.message.includes("HydroServer returned an error");

  if (isUrlError) {
    markField("url", "invalid", message);
    for (const field of credentialFields(server.auth_type)) {
      markField(field, "idle");
    }
    return;
  }

  if (result.invalid_field === "workspace_name") {
    markField("workspace_name", "invalid", message);
    if (server.auth_type === "userpass") {
      markField("username", "valid");
      markField("password", "valid");
    }
    return;
  }

  for (const field of credentialFields(server.auth_type)) {
    markField(field, "invalid", message);
  }
}

export async function runAuthSubmission<T>(params: {
  render: () => void;
  setSubmitting: (value: boolean) => void;
  action: () => Promise<T>;
}): Promise<T> {
  const { render, setSubmitting, action } = params;
  setSubmitting(true);
  render();

  try {
    return await action();
  } finally {
    setSubmitting(false);
    render();
  }
}
