import test from "node:test"
import assert from "node:assert/strict"

import {
  requiresDesktopServiceSetup,
  resolvePostAuthRoute,
  resolveAuthenticatedRoute,
  shouldBootstrapDesktopDaemon,
  shouldApplyDaemonConnectionState,
  shouldHydrateAuthDraftFromDaemon,
} from "../composables/useAppModel"

test("connected users with saved datasources default to the dashboard", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "welcome",
      hasSavedDatasources: true,
      pipelineReadyForMapping: false,
      serviceReady: true,
    }),
    "dashboard"
  )
})

test("connected users without saved datasources default to onboarding", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "welcome",
      hasSavedDatasources: false,
      pipelineReadyForMapping: false,
      serviceReady: true,
    }),
    "jobs-new"
  )
})

test("dashboard route redirects to onboarding when no datasources exist", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "dashboard",
      hasSavedDatasources: false,
      pipelineReadyForMapping: false,
      serviceReady: true,
    }),
    "jobs-new"
  )
})

test("users can still stay in onboarding even when datasources already exist", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "jobs-new",
      hasSavedDatasources: true,
      pipelineReadyForMapping: false,
      serviceReady: true,
    }),
    "jobs-new"
  )
})

test("connected users are sent to service setup when the background service is unavailable", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "dashboard",
      hasSavedDatasources: true,
      pipelineReadyForMapping: false,
      serviceReady: false,
    }),
    "service"
  )
})

test("post-auth redirect sends users with saved datasources to the dashboard", () => {
  assert.equal(
    resolvePostAuthRoute({
      hasSavedDatasources: true,
      serviceReady: true,
    }),
    "dashboard"
  )
})

test("post-auth redirect sends users without saved datasources to onboarding", () => {
  assert.equal(
    resolvePostAuthRoute({
      hasSavedDatasources: false,
      serviceReady: true,
    }),
    "jobs-new"
  )
})

test("desktop runtime blocks on service setup before authentication when service is unavailable", () => {
  assert.equal(
    requiresDesktopServiceSetup({
      tauriRuntime: true,
      serviceReady: false,
      daemonReady: false,
    }),
    true
  )
})

test("desktop runtime blocks on service setup when daemon bootstrap has not completed", () => {
  assert.equal(
    requiresDesktopServiceSetup({
      tauriRuntime: true,
      serviceReady: true,
      daemonReady: false,
    }),
    true
  )
})

test("browser runtime does not require OS service setup", () => {
  assert.equal(
    requiresDesktopServiceSetup({
      tauriRuntime: false,
      serviceReady: false,
      daemonReady: false,
    }),
    false
  )
})

test("desktop runtime skips daemon bootstrap until the service is ready", () => {
  assert.equal(
    shouldBootstrapDesktopDaemon({
      tauriRuntime: true,
      serviceStatus: {
        supported: true,
        installed: false,
        running: false,
        label: "",
        plist_path: "",
        executable_path: "",
        status_message: "",
      },
    }),
    false
  )
})

test("desktop runtime bootstraps the daemon once the service is running", () => {
  assert.equal(
    shouldBootstrapDesktopDaemon({
      tauriRuntime: true,
      serviceStatus: {
        supported: true,
        installed: true,
        running: true,
        label: "",
        plist_path: "",
        executable_path: "",
        status_message: "",
      },
    }),
    true
  )
})

test("daemon snapshots do not overwrite an auth draft with local edits", () => {
  assert.equal(
    shouldHydrateAuthDraftFromDaemon({
      authSubmitting: false,
      authDraftDirty: true,
    }),
    false
  )
})

test("daemon snapshots still hydrate a clean auth draft", () => {
  assert.equal(
    shouldHydrateAuthDraftFromDaemon({
      authSubmitting: false,
      authDraftDirty: false,
    }),
    true
  )
})

test("daemon snapshots do not downgrade connection state during auth submission", () => {
  assert.equal(
    shouldApplyDaemonConnectionState({
      authSubmitting: true,
      snapshotConnectionState: "not_configured",
    }),
    false
  )
})

test("connected daemon snapshots can still apply during auth submission", () => {
  assert.equal(
    shouldApplyDaemonConnectionState({
      authSubmitting: true,
      snapshotConnectionState: "connected",
    }),
    true
  )
})
