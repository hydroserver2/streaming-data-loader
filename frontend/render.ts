import { state, connected, onboardingRoute } from "./state";
import { getRouteFromHash, navigate } from "./router";
import {
  connectionIndicator,
  renderWelcome,
  renderSettings,
  renderFatalError,
  renderLoading,
} from "./components/auth";
import { renderDashboard } from "./components/dashboard";
import { renderOnboardingFile } from "./components/onboarding-file";
import { renderOnboardingMapping } from "./components/onboarding-mapping";

type ShellElements = {
  sidebar: HTMLElement;
  mainContent: HTMLElement;
  jobsLink: HTMLAnchorElement;
  settingsLink: HTMLAnchorElement;
  connectionDot: HTMLElement;
};

let _elements: ShellElements | null = null;
let _lastMarkup = "";

export function initRenderer(elements: ShellElements): void {
  _elements = elements;
}

export function render(): void {
  if (!_elements) throw new Error("Renderer not initialized.");
  const { sidebar, mainContent, jobsLink, settingsLink, connectionDot } = _elements;

  state.route = getRouteFromHash();
  let currentRoute = state.route;

  // Route guards: redirect to the correct page based on auth/job state.
  if (!state.loading && !state.bootstrapError) {
    if (!connected() && currentRoute !== "settings" && currentRoute !== "welcome") {
      navigate("welcome");
      currentRoute = "welcome";
    } else if (
      connected() &&
      state.jobs.length === 0 &&
      (currentRoute === "dashboard" || currentRoute === "welcome")
    ) {
      navigate("jobs-new");
      currentRoute = "jobs-new";
    }
  }

  // Shell surface: full-page welcome surface hides the sidebar.
  const inOnboarding = onboardingRoute(currentRoute);
  const showSidebar = !inOnboarding && !state.bootstrapError;
  const welcomeSurface = Boolean(
    state.loading || state.bootstrapError || inOnboarding
  );
  sidebar.classList.toggle("hidden", !showSidebar);
  mainContent.classList.toggle("main-content-welcome", welcomeSurface);
  document.body.classList.toggle("app-surface-welcome", welcomeSurface);

  // Nav active state.
  jobsLink.className =
    currentRoute === "dashboard" ? "nav-item nav-item-active" : "nav-item";
  settingsLink.className =
    currentRoute === "settings" ? "nav-item nav-item-active" : "nav-item";

  // Connection dot.
  const status = connectionIndicator();
  connectionDot.className = status.className;
  connectionDot.title = status.label;

  // Content.
  let markup: string;
  if (state.loading) {
    markup = renderLoading();
  } else if (state.bootstrapError) {
    markup = renderFatalError();
  } else if (currentRoute === "settings") {
    markup = renderSettings();
  } else if (currentRoute === "welcome") {
    markup = renderWelcome();
  } else if (currentRoute === "jobs-new") {
    markup =
      state.onboardingStep === "file-config"
        ? renderOnboardingFile()
        : renderOnboardingMapping();
  } else {
    markup = renderDashboard();
  }

  // Only write to the DOM when something changed.
  if (markup !== _lastMarkup) {
    mainContent.innerHTML = markup;
    _lastMarkup = markup;
  }
}
