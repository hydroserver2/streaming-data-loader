import test from "node:test"
import assert from "node:assert/strict"

import {
  createPipelineFieldStates,
  validatePipelineFieldsForSubmit,
  type PipelineFieldName,
} from "../pipeline-submit"
import { createEmptyPipelineForm } from "../composables/state"

function validateForm(params?: {
  hasPreview?: boolean
  previewHeaders?: string[]
  mutate?: ReturnType<typeof createEmptyPipelineForm>
}) {
  const form = params?.mutate ?? createEmptyPipelineForm()
  const fieldStates = createPipelineFieldStates()

  const valid = validatePipelineFieldsForSubmit({
    form,
    hasPreview: params?.hasPreview ?? true,
    previewHeaders: params?.previewHeaders ?? ["timestamp", "value"],
    markField: (field, nextState, message) => {
      fieldStates[field] = {
        state: nextState,
        message: message ?? null,
      }
    },
  })

  return { valid, fieldStates }
}

test("validatePipelineFieldsForSubmit requires a preview before continuing", () => {
  const form = createEmptyPipelineForm()
  form.filePath = "/tmp/preview.csv"
  form.timestamp.key = "timestamp"

  const { valid, fieldStates } = validateForm({
    hasPreview: false,
    mutate: form,
  })

  assert.equal(valid, false)
  assert.equal(fieldStates.file_path.state, "invalid")
})

test("validatePipelineFieldsForSubmit rejects timestamp keys outside the preview", () => {
  const form = createEmptyPipelineForm()
  form.filePath = "/tmp/preview.csv"
  form.timestamp.key = "missing_column"

  const { valid, fieldStates } = validateForm({ mutate: form })

  assert.equal(valid, false)
  assert.equal(fieldStates.timestamp_key.state, "invalid")
})

test("validatePipelineFieldsForSubmit enforces controlled timezone vocabularies", () => {
  const form = createEmptyPipelineForm()
  form.filePath = "/tmp/preview.csv"
  form.timestamp.key = "timestamp"
  form.timestamp.format = "naive"
  form.timestamp.timezoneMode = "daylightSavings"
  form.timestamp.timezone = "Mars/Olympus_Mons"

  const { valid, fieldStates } = validateForm({ mutate: form })

  assert.equal(valid, false)
  assert.equal(fieldStates.timezone.state, "invalid")
})

test("validatePipelineFieldsForSubmit accepts a valid index-based configuration", () => {
  const form = createEmptyPipelineForm()
  form.filePath = "/tmp/preview.csv"
  form.hasHeaderRow = false
  form.identifierType = "index"
  form.dataStartRow = 1
  form.timestamp.key = "1"
  form.timestamp.format = "naive"
  form.timestamp.timezoneMode = "fixedOffset"
  form.timestamp.timezone = "-0700"

  const { valid, fieldStates } = validateForm({
    mutate: form,
    previewHeaders: ["Column 1", "Column 2"],
  })

  assert.equal(valid, true)
  for (const field of Object.keys(fieldStates) as PipelineFieldName[]) {
    assert.notEqual(fieldStates[field].state, "invalid")
  }
})
