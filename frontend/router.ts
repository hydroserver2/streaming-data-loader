export type AppRoute = "welcome" | "service" | "dashboard" | "jobs-new" | "jobs-new-mapping"

const DEFAULT_ROUTE: AppRoute = "welcome"

function currentHash(): string {
  if (typeof window === "undefined") return ""
  return window.location.hash
}

export function getRouteFromHash(hash = currentHash()): AppRoute {
  const normalized = hash.replace(/^#/, "").trim()

  switch (normalized) {
    case "service":
      return "service"
    case "dashboard":
      return "dashboard"
    case "jobs/new/mapping":
      return "jobs-new-mapping"
    case "jobs/new":
      return "jobs-new"
    case "welcome":
    case "":
      return "welcome"
    default:
      return DEFAULT_ROUTE
  }
}

export function routeHref(route: AppRoute): string {
  switch (route) {
    case "service":
      return "#service"
    case "dashboard":
      return "#dashboard"
    case "jobs-new-mapping":
      return "#jobs/new/mapping"
    case "jobs-new":
      return "#jobs/new"
    case "welcome":
    default:
      return "#welcome"
  }
}

export function navigate(route: AppRoute): void {
  if (typeof window === "undefined") return

  const nextHref = routeHref(route)
  if (window.location.hash !== nextHref) {
    window.location.hash = nextHref
  }
}
