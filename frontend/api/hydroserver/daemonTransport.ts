import type { DaemonConnectionInfo, DaemonStatusSnapshot } from './types'
import {
  formatErrorDetail,
  invokeCommand,
  isTauriRuntime,
  normalizeError,
} from '../runtime'

let daemonConnectionPromise: Promise<DaemonConnectionInfo> | null = null

function resetDaemonConnection(): void {
  daemonConnectionPromise = null
}

export function disconnectDaemonConnection(): void {
  resetDaemonConnection()
}

export async function getDaemonConnection(): Promise<DaemonConnectionInfo> {
  if (!isTauriRuntime()) {
    throw new Error(
      'The daemon connection is only available in the desktop app.'
    )
  }

  if (!daemonConnectionPromise) {
    daemonConnectionPromise = invokeCommand<DaemonConnectionInfo>(
      'get_daemon_connection'
    ).catch((error) => {
      resetDaemonConnection()
      throw error
    })
  }

  return daemonConnectionPromise
}

export async function daemonCommand<T>(
  command: string,
  payload?: Record<string, unknown>
): Promise<T> {
  const connection = await getDaemonConnection()
  const baseUrl = connection.base_url.replace(/\/$/, '')

  try {
    const response = await fetch(`${baseUrl}/api/commands/${command}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${connection.token}`,
      },
      body: JSON.stringify(payload ?? {}),
    })

    if (!response.ok) {
      let detail = `Request failed with status ${response.status}`

      try {
        const body = (await response.json()) as { detail?: unknown }
        const formattedDetail = formatErrorDetail(body.detail)
        if (formattedDetail) {
          detail = formattedDetail
        }
      } catch {
        // Ignore JSON parsing errors for non-JSON error responses.
      }

      throw new Error(detail)
    }

    return (await response.json()) as T
  } catch (error) {
    resetDaemonConnection()
    throw normalizeError(error)
  }
}

export function subscribeToDaemonStatus(handlers: {
  onStatus: (snapshot: DaemonStatusSnapshot) => void
  onError?: (error: Error) => void
}): () => void {
  if (!isTauriRuntime()) {
    return () => undefined
  }

  let closed = false
  let reconnectTimer: number | null = null
  let eventSource: EventSource | null = null

  const connect = async () => {
    try {
      const connection = await getDaemonConnection()
      if (closed) return

      const url = new URL(
        `${connection.base_url.replace(/\/$/, '')}/api/status`
      )
      url.searchParams.set('access_token', connection.token)

      eventSource = new EventSource(url.toString())
      eventSource.addEventListener('status', (event) => {
        if (!(event instanceof MessageEvent)) return
        try {
          handlers.onStatus(JSON.parse(event.data) as DaemonStatusSnapshot)
        } catch (error) {
          handlers.onError?.(normalizeError(error))
        }
      })
      eventSource.onerror = () => {
        eventSource?.close()
        eventSource = null
        resetDaemonConnection()
        handlers.onError?.(
          new Error('Lost connection to the local daemon. Retrying...')
        )
        if (!closed) {
          reconnectTimer = window.setTimeout(() => {
            reconnectTimer = null
            void connect()
          }, 1000)
        }
      }
    } catch (error) {
      handlers.onError?.(normalizeError(error))
      if (!closed) {
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null
          void connect()
        }, 1000)
      }
    }
  }

  void connect()

  return () => {
    closed = true
    if (reconnectTimer !== null) {
      window.clearTimeout(reconnectTimer)
    }
    eventSource?.close()
  }
}
