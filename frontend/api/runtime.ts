export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as Window & typeof globalThis)
  )
}

export function formatErrorDetail(detail: unknown): string | null {
  if (typeof detail === "string" && detail.trim()) {
    return detail
  }

  if (Array.isArray(detail)) {
    const firstMessage = detail
      .map((item) => {
        if (typeof item === "string") return item
        if (
          item &&
          typeof item === "object" &&
          "msg" in item &&
          typeof item.msg === "string"
        ) {
          return item.msg
        }
        return null
      })
      .find(Boolean)

    return firstMessage ?? null
  }

  if (detail && typeof detail === "object") {
    if ("msg" in detail && typeof detail.msg === "string") {
      return detail.msg
    }

    try {
      return JSON.stringify(detail)
    } catch {
      return null
    }
  }

  return null
}

export function normalizeError(error: unknown): Error {
  if (error instanceof Error) return error
  if (typeof error === "string" && error.trim()) return new Error(error)
  return new Error("Request failed.")
}

export async function invokeCommand<T>(
  command: string,
  payload?: Record<string, unknown>
): Promise<T> {
  try {
    const { invoke } = await import("@tauri-apps/api/core")
    return await invoke<T>(command, payload)
  } catch (error) {
    throw normalizeError(error)
  }
}
