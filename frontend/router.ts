export type AppRoute = "welcome" | "jobs-new"

const DEFAULT_ROUTE: AppRoute = "welcome"

function currentHash(): string {
  if (typeof window === "undefined") return ""
  return window.location.hash
}

export function getRouteFromHash(hash = currentHash()): AppRoute {
  const normalized = hash.replace(/^#/, "").trim()

  switch (normalized) {
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
