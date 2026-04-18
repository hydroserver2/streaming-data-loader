import test from "node:test"
import assert from "node:assert/strict"

import {
  requiresDesktopServiceSetup,
  resolveAuthenticatedRoute,
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
