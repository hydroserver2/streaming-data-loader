export type AppRoute = "dashboard" | "settings" | "welcome" | "jobs-new"

const DEFAULT_ROUTE: AppRoute = "dashboard"

export function getRouteFromHash(hash = window.location.hash): AppRoute {
  const normalized = hash.replace(/^#/, "").trim()

  switch (normalized) {
    case "settings":
      return "settings"
    case "welcome":
      return "welcome"
    case "jobs/new":
      return "jobs-new"
    case "dashboard":
    case "":
      return DEFAULT_ROUTE
    default:
      return DEFAULT_ROUTE
  }
}

export function routeHref(route: AppRoute): string {
  switch (route) {
    case "settings":
      return "#settings"
    case "welcome":
      return "#welcome"
    case "jobs-new":
      return "#jobs/new"
    case "dashboard":
    default:
      return "#dashboard"
  }
}

export function navigate(route: AppRoute): void {
  const nextHref = routeHref(route)
  if (window.location.hash !== nextHref) {
    window.location.hash = nextHref
  }
}
