import type {
  CsvTransformerSettings,
  CsvTransformerTimestampSettings,
} from "../api"
import {
  createEmptyPipelineForm,
  type PipelineFormState,
  type PipelineTimezoneType,
} from "./state"

function normalizePipelineTimezoneType(
  timestampType: PipelineFormState["timestampType"],
  timezoneType: PipelineTimezoneType
): PipelineTimezoneType {
  if (timestampType === "custom" && timezoneType === "") {
    return "utc"
  }

  return timezoneType
}

function toTimezoneMode(
  timestampType: PipelineFormState["timestampType"],
  timezoneType: PipelineTimezoneType
): CsvTransformerTimestampSettings["timezoneMode"] {
  const normalizedTimezoneType = normalizePipelineTimezoneType(
    timestampType,
    timezoneType
  )

  switch (normalizedTimezoneType) {
    case "offset":
      return "fixedOffset"
    case "iana":
      return "daylightSavings"
    case "utc":
      return "utc"
    case "":
    default:
      return "embeddedOffset"
  }
}

function toTimestampFormat(
  timestampType: PipelineFormState["timestampType"],
  timezoneType: PipelineTimezoneType
): CsvTransformerTimestampSettings["format"] {
  if (timestampType === "custom") {
    return "custom"
  }

  return normalizePipelineTimezoneType(timestampType, timezoneType) === ""
    ? "ISO8601"
    : "naive"
}

function toPipelineTimezoneType(
  format: CsvTransformerTimestampSettings["format"] | undefined,
  timezoneMode: CsvTransformerTimestampSettings["timezoneMode"] | undefined
): PipelineTimezoneType {
  if (format === "ISO8601" || timezoneMode === "embeddedOffset") {
    return ""
  }

  switch (timezoneMode) {
    case "fixedOffset":
      return "offset"
    case "daylightSavings":
      return "iana"
    case "utc":
    default:
      return "utc"
  }
}

export function serializePipelineFormToCsvTransformerSettings(
  form: PipelineFormState
): CsvTransformerSettings {
  const timezoneType = normalizePipelineTimezoneType(
    form.timestampType,
    form.timezoneType
  )
  const timestampFormat = toTimestampFormat(form.timestampType, timezoneType)
  const timestamp: CsvTransformerTimestampSettings = {
    key: form.timestampKey,
    format: timestampFormat,
    timezoneMode: toTimezoneMode(form.timestampType, timezoneType),
  }

  if (timestampFormat === "custom") {
    timestamp.customFormat =
      form.timestampFormat.trim() || "%Y-%m-%d %H:%M:%S"
  }

  if (timezoneType === "offset" || timezoneType === "iana") {
    timestamp.timezone = form.timezone.trim()
  }

  return {
    headerRow:
      form.hasHeaderRow && form.identifierType === "name" ? form.headerRow : null,
    dataStartRow: form.dataStartRow,
    delimiter: form.delimiter,
    identifierType: form.identifierType,
    timestamp,
  }
}

export function deserializeCsvTransformerSettingsToPipelineForm(
  settings: Partial<CsvTransformerSettings>,
  filePath = ""
): PipelineFormState {
  const form = createEmptyPipelineForm()
  const timestamp = settings.timestamp
  const headerRow = settings.headerRow ?? null
  const dataStartRow = settings.dataStartRow ?? form.dataStartRow
  const timestampType =
    timestamp?.format === "custom" ? "custom" : form.timestampType
  const timezoneType = normalizePipelineTimezoneType(
    timestampType,
    toPipelineTimezoneType(timestamp?.format, timestamp?.timezoneMode)
  )

  form.filePath = filePath
  form.hasHeaderRow = headerRow !== null || dataStartRow > 1
  form.headerRow = headerRow ?? Math.max(1, dataStartRow - 1)
  form.dataStartRow = dataStartRow
  form.delimiter = settings.delimiter ?? form.delimiter
  form.identifierType = settings.identifierType ?? form.identifierType
  form.timestampKey = timestamp?.key ?? form.timestampKey
  form.timestampType = timestampType
  form.timestampFormat = timestamp?.customFormat ?? ""
  form.timezoneType = timezoneType
  form.timezone = timestamp?.timezone ?? ""

  return form
}

export { normalizePipelineTimezoneType }
