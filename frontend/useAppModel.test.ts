import test from "node:test"
import assert from "node:assert/strict"

import { resolveAuthenticatedRoute } from "./composables/useAppModel"

test("connected users with saved datasources default to the dashboard", () => {
  assert.equal(
    resolveAuthenticatedRoute({
      route: "welcome",
      hasSavedDatasources: true,
      pipelineReadyForMapping: false,
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
    }),
    "jobs-new"
  )
})
