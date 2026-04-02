export function formatRelativeTime(value: string | null): string {
  if (!value) {
    return "Never run"
  }

  const timestamp = new Date(value)
  const deltaSeconds = Math.max(0, Math.floor((Date.now() - timestamp.getTime()) / 1000))

  if (deltaSeconds < 60) {
    return "Just now"
  }

  const deltaMinutes = Math.floor(deltaSeconds / 60)
  if (deltaMinutes < 60) {
    return `${deltaMinutes} min ago`
  }

  const deltaHours = Math.floor(deltaMinutes / 60)
  if (deltaHours < 24) {
    return deltaHours === 1 ? "1 hour ago" : `${deltaHours} hours ago`
  }

  if (deltaHours < 48) {
    return "Yesterday"
  }

  const deltaDays = Math.floor(deltaHours / 24)
  return `${deltaDays} days ago`
}

export function formatSchedule(minutes: number): string {
  if (minutes < 60) {
    return `Every ${minutes} min`
  }

  const hours = minutes / 60
  if (Number.isInteger(hours)) {
    return `Every ${hours} hour${hours === 1 ? "" : "s"}`
  }

  return `Every ${minutes} min`
}

export function shortenPath(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean)
  if (segments.length <= 2) {
    return path
  }
  return `${segments[segments.length - 2]} / ${segments[segments.length - 1]}`
}
