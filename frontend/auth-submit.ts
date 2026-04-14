import type {
  AuthType,
  ConnectionTestResponse,
  ServerConfig,
} from "./api";

export type AuthFieldName = "url" | "api_key" | "username" | "password";

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

  if (authType === "apikey") {
    authFieldStates.username = emptyFieldValidationState();
    authFieldStates.password = emptyFieldValidationState();
  } else {
    authFieldStates.api_key = emptyFieldValidationState();
  }
}

export function credentialFields(authType: AuthType): AuthFieldName[] {
  return authType === "userpass" ? ["username", "password"] : ["api_key"];
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
    markField("url", "invalid", "Enter the HydroServer URL.");
    valid = false;
  } else if (!isValidHttpUrl(server.url)) {
    markField("url", "invalid", "Enter a full http:// or https:// URL.");
    valid = false;
  } else {
    markField("url", "valid");
  }

  if (server.auth_type === "apikey") {
    if (!server.api_key) {
      markField("api_key", "invalid", "Enter the API key.");
      valid = false;
    } else {
      markField("api_key", "valid");
    }
  } else {
    if (!server.username) {
      markField("username", "invalid", "Enter the username.");
      valid = false;
    } else {
      markField("username", "valid");
    }

    if (!server.password) {
      markField("password", "invalid", "Enter the password.");
      valid = false;
    } else {
      markField("password", "valid");
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
