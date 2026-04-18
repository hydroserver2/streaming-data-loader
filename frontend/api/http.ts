import { apiBaseUrl } from "../config"
import { formatErrorDetail } from "./runtime"

function buildApiUrl(path: string): string {
  return `${apiBaseUrl.replace(/\/$/, "")}${path}`
}

export async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(buildApiUrl(path), {
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  })

  if (!response.ok) {
    let detail = `Request failed with status ${response.status}`

    try {
      const payload = (await response.json()) as { detail?: unknown }
      const formattedDetail = formatErrorDetail(payload.detail)
      if (formattedDetail) {
        detail = formattedDetail
      }
    } catch {
      // Ignore JSON parsing errors for non-JSON error responses.
    }

    throw new Error(detail)
  }

  return (await response.json()) as T
}
