import type { FieldValidationState } from "./auth-submit"
import {
  DST_AWARE_TIMEZONES,
  FIXED_OFFSET_TIMEZONES,
} from "./models/timestamp"
import type { PipelineFormState } from "./composables/state"

export type PipelineFieldName =
  | "file_path"
  | "header_row"
  | "data_start_row"
  | "timestamp_key"
  | "custom_timestamp_format"
  | "timezone"

export type PipelineFieldStates = Record<PipelineFieldName, FieldValidationState>

export function createPipelineFieldStates(): PipelineFieldStates {
  return {
    file_path: emptyFieldValidationState(),
    header_row: emptyFieldValidationState(),
    data_start_row: emptyFieldValidationState(),
    timestamp_key: emptyFieldValidationState(),
    custom_timestamp_format: emptyFieldValidationState(),
    timezone: emptyFieldValidationState(),
  }
}

export function resetPipelineFieldStates(
  fieldStates: PipelineFieldStates
): void {
  for (const field of Object.keys(fieldStates) as PipelineFieldName[]) {
    fieldStates[field] = emptyFieldValidationState()
  }
}

export function validatePipelineFieldsForSubmit(params: {
  form: PipelineFormState
  hasPreview: boolean
  previewHeaders: string[]
  markField: (
    field: PipelineFieldName,
    nextState: FieldValidationState["state"],
    message?: string | null
  ) => void
}): boolean {
  const { form, hasPreview, previewHeaders, markField } = params
  let valid = true

  if (!form.filePath.trim()) {
    markField("file_path", "invalid", "Choose a CSV file path.")
    valid = false
  } else if (!hasPreview) {
    markField(
      "file_path",
      "invalid",
      "Load a CSV preview before continuing to mapping."
    )
    valid = false
  } else {
    markField("file_path", "valid")
  }

  if (form.identifierType === "name") {
    if (!Number.isInteger(form.headerRow) || form.headerRow <= 0) {
      markField("header_row", "invalid", "Enter a header row number above 0.")
      valid = false
    } else if (form.headerRow >= form.dataStartRow) {
      markField(
        "header_row",
        "invalid",
        "Header row must be less than the data start row."
      )
      valid = false
    } else {
      markField("header_row", "valid")
    }
  } else {
    markField("header_row", "valid")
  }

  if (!Number.isInteger(form.dataStartRow) || form.dataStartRow <= 0) {
    markField("data_start_row", "invalid", "Enter a data start row above 0.")
    valid = false
  } else if (
    form.identifierType === "name" &&
    form.dataStartRow <= form.headerRow
  ) {
    markField(
      "data_start_row",
      "invalid",
      "Data start row must be greater than the header row."
    )
    valid = false
  } else {
    markField("data_start_row", "valid")
  }

  if (form.identifierType === "index") {
    const timestampIndex = Number(form.timestamp.key)
    if (!Number.isInteger(timestampIndex) || timestampIndex <= 0) {
      markField(
        "timestamp_key",
        "invalid",
        "Enter a positive timestamp column number."
      )
      valid = false
    } else if (previewHeaders.length > 0 && timestampIndex > previewHeaders.length) {
      markField(
        "timestamp_key",
        "invalid",
        "Choose a timestamp column that exists in the preview."
      )
      valid = false
    } else {
      markField("timestamp_key", "valid")
    }
  } else if (!form.timestamp.key.trim()) {
    markField("timestamp_key", "invalid", "Choose a timestamp column.")
    valid = false
  } else if (
    previewHeaders.length > 0 &&
    !previewHeaders.includes(form.timestamp.key)
  ) {
    markField(
      "timestamp_key",
      "invalid",
      "Choose a timestamp column that exists in the preview."
    )
    valid = false
  } else {
    markField("timestamp_key", "valid")
  }

  if (form.timestamp.format === "custom") {
    if (!(form.timestamp.customFormat ?? "").trim()) {
      markField(
        "custom_timestamp_format",
        "invalid",
        "Enter the custom timestamp format."
      )
      valid = false
    } else {
      markField("custom_timestamp_format", "valid")
    }
  } else {
    markField("custom_timestamp_format", "valid")
  }

  if (
    form.timestamp.timezoneMode === "fixedOffset" ||
    form.timestamp.timezoneMode === "daylightSavings"
  ) {
    const allowedTimezones =
      form.timestamp.timezoneMode === "fixedOffset"
        ? FIXED_OFFSET_TIMEZONES
        : DST_AWARE_TIMEZONES
    const timezone = form.timestamp.timezone ?? ""

    if (!timezone) {
      markField("timezone", "invalid", "Choose a timezone value.")
      valid = false
    } else if (!allowedTimezones.some((option) => option.value === timezone)) {
      markField("timezone", "invalid", "Choose a timezone from the list.")
      valid = false
    } else {
      markField("timezone", "valid")
    }
  } else {
    markField("timezone", "valid")
  }

  return valid
}

function emptyFieldValidationState(): FieldValidationState {
  return { state: "idle", message: null }
}
