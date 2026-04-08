<script setup lang="ts">
import { computed } from "vue"

import { useAppModel } from "../composables/useAppModel"
import type { PipelineIdentifierType } from "../composables/state"
import type { PipelineFieldName } from "../pipeline-submit"
import {
  DST_AWARE_TIMEZONES,
  FIXED_OFFSET_TIMEZONES,
  TIMESTAMP_FORMATS,
  type TimezoneMode,
} from "../models/timestamp"

const model = useAppModel()

const delimiterOptions = [
  { value: ",", label: "Comma (,)" },
  { value: ";", label: "Semicolon (;)" },
  { value: "\t", label: "Tab" },
  { value: "|", label: "Pipe (|)" },
  { value: " ", label: "Space" },
] as const

const timezoneModeOptions = [
  { value: "utc", label: "UTC" },
  { value: "fixedOffset", label: "Fixed offset" },
  { value: "daylightSavings", label: "Daylight savings aware" },
] as const

const timestampKeyLabel = computed(() =>
  model.state.pipelineForm.identifierType === "index"
    ? "Timestamp column number"
    : "Timestamp column name"
)

const timestampKeyHint = computed(() =>
  model.state.pipelineForm.identifierType === "index"
    ? "Pick the 1-based column number that contains timestamps."
    : "Pick the header name that contains timestamps."
)

const timestampOptions = computed(() =>
  model.previewHeaders.value.map((header, index) => ({
    id: String(index + 1),
    value:
      model.state.pipelineForm.identifierType === "index"
        ? String(index + 1)
        : header,
    label:
      model.state.pipelineForm.identifierType === "index"
        ? `${index + 1} · ${header}`
        : header,
  }))
)

const timezoneLabel = computed(() =>
  model.state.pipelineForm.timestamp.timezoneMode === "fixedOffset"
    ? "UTC offset"
    : "Timezone"
)

const timezoneValueHint = computed(() =>
  model.state.pipelineForm.timestamp.timezoneMode === "fixedOffset"
    ? "Select a fixed UTC offset to apply to the timestamp column."
    : "Select an IANA timezone such as America/Denver."
)

const timezoneModeHint = computed(() => {
  if (model.state.pipelineForm.timestamp.format === "custom") {
    return "Custom formats must be interpreted as UTC, a fixed offset, or an IANA timezone."
  }

  return "Timezone-naive timestamps need an explicit timezone rule."
})

const timezoneOptions = computed(() =>
  model.state.pipelineForm.timestamp.timezoneMode === "fixedOffset"
    ? FIXED_OFFSET_TIMEZONES
    : DST_AWARE_TIMEZONES
)

function fieldError(field: PipelineFieldName): string | null {
  const state = model.state.pipelineFieldStates[field]
  return state.state === "invalid" ? state.message : null
}

function updateIdentifierType(event: Event): void {
  model.setPipelineIdentifierType(
    (event.target as HTMLSelectElement).value as PipelineIdentifierType
  )
}

function updateTimezoneMode(event: Event): void {
  model.updatePipelineField(
    "timezone_mode",
    (event.target as HTMLSelectElement).value as TimezoneMode
  )
}
</script>

<template>
  <section class="transformer-settings">
    <article class="pipeline-subcard transformer-section">
      <h2 class="section-title">CSV structure</h2>

      <label class="preview-toggle transformer-toggle">
        <input
          class="preview-toggle-input"
          type="checkbox"
          :checked="model.state.pipelineForm.hasHeaderRow"
          @change="
            model.setPipelineHasHeaderRow(
              ($event.target as HTMLInputElement).checked
            )
          "
        />
        <span class="preview-toggle-label">File has a header row</span>
      </label>

      <div class="split-fields">
        <label class="field">
          <span class="label">Delimiter</span>
          <select
            class="input"
            :value="model.state.pipelineForm.delimiter"
            @change="
              model.updatePipelineField(
                'delimiter',
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="option in delimiterOptions"
              :key="option.label"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <span class="field-hint">
            The preview re-parses immediately when you change this.
          </span>
        </label>

        <label class="field">
          <span class="label">Column identifiers</span>
          <select
            class="input"
            :value="model.state.pipelineForm.identifierType"
            @change="updateIdentifierType"
          >
            <option :disabled="!model.state.pipelineForm.hasHeaderRow" value="name">
              Header names
            </option>
            <option value="index">Column numbers</option>
          </select>
          <span class="field-hint">
            Name mode requires a real header row. Index mode uses 1-based column
            numbers.
          </span>
        </label>

        <label class="field">
          <span class="label">Header row number</span>
          <input
            class="input"
            :disabled="!model.state.pipelineForm.hasHeaderRow"
            min="1"
            step="1"
            type="number"
            :value="model.state.pipelineForm.headerRow"
            @input="
              model.updatePipelineField(
                'header_row',
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">
            Pick the 1-based line that contains the column names.
          </span>
          <p v-if="fieldError('header_row')" class="field-error">
            {{ fieldError("header_row") }}
          </p>
        </label>

        <label class="field">
          <span class="label">Data start row number</span>
          <input
            class="input"
            :min="model.state.pipelineForm.hasHeaderRow ? 2 : 1"
            step="1"
            type="number"
            :value="model.state.pipelineForm.dataStartRow"
            @input="
              model.updatePipelineField(
                'data_start_row',
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">
            Pick the 1-based line where observation values begin.
          </span>
          <p v-if="fieldError('data_start_row')" class="field-error">
            {{ fieldError("data_start_row") }}
          </p>
        </label>
      </div>
    </article>

    <article class="pipeline-subcard transformer-section">
      <h2 class="section-title">Timestamp parsing</h2>

      <div class="split-fields">
        <label class="field">
          <span class="label">{{ timestampKeyLabel }}</span>
          <select
            class="input"
            :key="model.state.pipelineForm.identifierType"
            :value="model.state.pipelineForm.timestamp.key"
            @change="
              model.updatePipelineField(
                'timestamp_key',
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="option in timestampOptions"
              :key="option.id"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <span class="field-hint">{{ timestampKeyHint }}</span>
          <p v-if="fieldError('timestamp_key')" class="field-error">
            {{ fieldError("timestamp_key") }}
          </p>
        </label>

        <label class="field">
          <span class="label">Timestamp format</span>
          <select
            class="input"
            :value="model.state.pipelineForm.timestamp.format"
            @change="
              model.updatePipelineField(
                'timestamp_format',
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="option in TIMESTAMP_FORMATS"
              :key="option.value"
              :value="option.value"
            >
              {{ option.text }}
            </option>
          </select>
          <span class="field-hint">
            Choose ISO 8601 when timestamps already contain their timezone
            offset. Use a custom format only when the values don't match the
            built-in options.
          </span>
        </label>

        <label
          v-if="model.state.pipelineForm.timestamp.format === 'custom'"
          class="field transformer-field-span"
        >
          <span class="label">Custom timestamp format</span>
          <input
            class="input"
            type="text"
            placeholder="%Y-%m-%d %H:%M:%S"
            :value="model.state.pipelineForm.timestamp.customFormat ?? ''"
            @input="
              model.updatePipelineField(
                'custom_timestamp_format',
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">
            Example: <code>%Y-%m-%d %H:%M:%S</code>
          </span>
          <p v-if="fieldError('custom_timestamp_format')" class="field-error">
            {{ fieldError("custom_timestamp_format") }}
          </p>
        </label>

        <label
          v-if="model.state.pipelineForm.timestamp.format !== 'ISO8601'"
          class="field"
        >
          <span class="label">Timezone</span>
          <select
            class="input"
            :value="model.state.pipelineForm.timestamp.timezoneMode"
            @change="updateTimezoneMode"
          >
            <option
              v-for="option in timezoneModeOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <span class="field-hint">{{ timezoneModeHint }}</span>
        </label>

        <label
          v-if="
            model.state.pipelineForm.timestamp.timezoneMode === 'fixedOffset' ||
            model.state.pipelineForm.timestamp.timezoneMode ===
              'daylightSavings'
          "
          class="field"
        >
          <span class="label">{{ timezoneLabel }}</span>
          <select
            class="input"
            :value="model.state.pipelineForm.timestamp.timezone"
            @change="
              model.updatePipelineField(
                'timezone',
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="option in timezoneOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.title }}
            </option>
          </select>
          <span class="field-hint">{{ timezoneValueHint }}</span>
          <p v-if="fieldError('timezone')" class="field-error">
            {{ fieldError("timezone") }}
          </p>
        </label>
      </div>

      <div class="button-row button-row-end">
        <button class="btn-primary" type="button" @click="model.submitPipelineConfig()">
          Validate and Continue
        </button>
      </div>
    </article>
  </section>
</template>
