<script setup lang="ts">
import { computed } from "vue";

import { useAppModel } from "../composables/useAppModel";
import type {
  PipelineIdentifierType,
  PipelineTimezoneType,
} from "../composables/state";

const model = useAppModel();

const delimiterOptions = [
  { value: ",", label: "Comma (,)" },
  { value: ";", label: "Semicolon (;)" },
  { value: "\t", label: "Tab" },
  { value: "|", label: "Pipe (|)" },
  { value: " ", label: "Space" },
] as const;

const timestampTypeOptions = [
  { value: "iso", label: "ISO / inferred" },
  { value: "custom", label: "Custom format" },
] as const;

const timezoneTypeOptions = [
  { value: "", label: "Embedded / auto" },
  { value: "utc", label: "Treat as UTC" },
  { value: "offset", label: "Fixed UTC offset" },
  { value: "iana", label: "IANA timezone" },
] as const;

const timestampKeyLabel = computed(() =>
  model.state.pipelineForm.identifierType === "index"
    ? "Timestamp column number"
    : "Timestamp column name"
);

const timestampKeyHint = computed(() =>
  model.state.pipelineForm.identifierType === "index"
    ? "Pick the 1-based column number that contains timestamps."
    : "Pick the header name that contains timestamps."
);

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
);

const timezoneLabel = computed(() =>
  model.state.pipelineForm.timezoneType === "offset"
    ? "UTC offset"
    : "Timezone"
);

const timezoneHint = computed(() => {
  if (model.state.pipelineForm.timezoneType === "offset") {
    return "Use ±HHMM or ±HH:MM, for example -0700 or -07:00."
  }

  return "Use a valid IANA timezone such as America/Denver."
});

function updateIdentifierType(event: Event): void {
  model.setPipelineIdentifierType(
    (event.target as HTMLSelectElement).value as PipelineIdentifierType
  );
}

function updateTimezoneType(event: Event): void {
  model.updatePipelineField(
    "timezone_type",
    (event.target as HTMLSelectElement).value as PipelineTimezoneType
  );
}
</script>

<template>
  <section class="transformer-settings">
    <article class="pipeline-subcard transformer-section">
      <div class="transformer-section-header">
        <p class="eyebrow">Transformer</p>
        <h2 class="section-title">CSV structure</h2>
        <p class="section-copy">
          Auto-detected structure lands here first. Adjust it if the preview
          shows the wrong rows or columns.
        </p>
      </div>

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
        </label>
      </div>
    </article>

    <article class="pipeline-subcard transformer-section">
      <div class="transformer-section-header">
        <p class="eyebrow">Transformer</p>
        <h2 class="section-title">Timestamp parsing</h2>
        <p class="section-copy">
          Preview selections stay synchronized with these fields, so you can use
          either the form or the table.
        </p>
      </div>

      <div class="split-fields">
        <label class="field">
          <span class="label">{{ timestampKeyLabel }}</span>
          <select
            class="input"
            :key="model.state.pipelineForm.identifierType"
            :value="model.state.pipelineForm.timestampKey"
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
        </label>

        <label class="field">
          <span class="label">Timestamp type</span>
          <select
            class="input"
            :value="model.state.pipelineForm.timestampType"
            @change="
              model.updatePipelineField(
                'timestamp_type',
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="option in timestampTypeOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <span class="field-hint">
            Use a custom format only when the timestamp values are not standard
            ISO strings.
          </span>
        </label>

        <label
          v-if="model.state.pipelineForm.timestampType === 'custom'"
          class="field transformer-field-span"
        >
          <span class="label">Custom timestamp format</span>
          <input
            class="input"
            type="text"
            placeholder="%Y-%m-%d %H:%M:%S"
            :value="model.state.pipelineForm.timestampFormat"
            @input="
              model.updatePipelineField(
                'timestamp_format',
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">
            Example: <code>%Y-%m-%d %H:%M:%S</code>
          </span>
        </label>

        <label class="field">
          <span class="label">Timezone handling</span>
          <select
            class="input"
            :value="model.state.pipelineForm.timezoneType"
            @change="updateTimezoneType"
          >
            <option
              v-for="option in timezoneTypeOptions"
              :key="option.label"
              :value="option.value"
            >
              {{ option.label }}
            </option>
          </select>
          <span class="field-hint">
            Leave this on embedded / auto when timestamps already include their
            own timezone offset.
          </span>
        </label>

        <label
          v-if="
            model.state.pipelineForm.timezoneType === 'offset' ||
            model.state.pipelineForm.timezoneType === 'iana'
          "
          class="field"
        >
          <span class="label">{{ timezoneLabel }}</span>
          <input
            class="input"
            :placeholder="
              model.state.pipelineForm.timezoneType === 'offset'
                ? '-07:00'
                : 'America/Denver'
            "
            type="text"
            :value="model.state.pipelineForm.timezone"
            @input="
              model.updatePipelineField(
                'timezone',
                ($event.target as HTMLInputElement).value
              )
            "
          />
          <span class="field-hint">{{ timezoneHint }}</span>
        </label>
      </div>
    </article>
  </section>
</template>
