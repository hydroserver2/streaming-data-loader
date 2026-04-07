<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"

import { PREVIEW_PAGE_SIZE, useAppModel, type PreviewRowSelectionTarget } from "../composables/useAppModel"

type RowDragState = {
  target: PreviewRowSelectionTarget
  originLineNumber: number
  lineNumber: number
  pointerId: number
  startClientY: number
  currentClientY: number
  moved: boolean
}

type ColumnDragState = {
  originColumnName: string
  columnName: string
  pointerId: number
  startClientX: number
  currentClientX: number
  moved: boolean
}

const model = useAppModel()

const rootRef = ref<HTMLElement | null>(null)
const rowDrag = ref<RowDragState | null>(null)
const columnDrag = ref<ColumnDragState | null>(null)
const suppressHandleClick = ref(false)

const headers = computed(() => model.previewHeaders.value)
const rows = computed(() =>
  model.parsedPreviewRows.value.map((row, index) => ({
    lineNumber: index + 1,
    row,
  }))
)
const shownLines = computed(() => model.state.pipelinePreview?.raw_lines.length ?? 0)
const nextPageSize = computed(() => {
  if (!model.state.pipelinePreview) {
    return PREVIEW_PAGE_SIZE
  }

  const remainingLines = Math.max(
    model.state.pipelinePreview.total_lines - shownLines.value,
    0
  )
  return Math.min(PREVIEW_PAGE_SIZE, remainingLines)
})
const displayHeaderLine = computed(() => {
  if (rowDrag.value?.target === "header-row") {
    return rowDrag.value.lineNumber
  }
  return model.previewHandleLine("header-row")
})
const displayDataStartLine = computed(() => {
  if (rowDrag.value?.target === "data-start-row") {
    return rowDrag.value.lineNumber
  }
  return model.previewHandleLine("data-start-row")
})
const displayTimestampColumn = computed(
  () => columnDrag.value?.columnName ?? model.activeTimestampColumn()
)
const previewFileName = computed(
  () => model.state.pipelineForm.filePath.split(/[\\/]/).filter(Boolean).at(-1) ?? ""
)

function handleOffsetStyle(target: PreviewRowSelectionTarget, lineNumber: number) {
  if (
    rowDrag.value &&
    rowDrag.value.target === target &&
    rowDrag.value.originLineNumber === lineNumber
  ) {
    return {
      transform: `translateY(${rowDrag.value.currentClientY - rowDrag.value.startClientY}px)`,
      willChange: "transform",
    }
  }

  return undefined
}

function timestampHandleStyle(columnName: string) {
  if (
    columnDrag.value &&
    columnDrag.value.originColumnName === columnName
  ) {
    return {
      transform: `translateX(${columnDrag.value.currentClientX - columnDrag.value.startClientX}px)`,
      willChange: "transform",
    }
  }

  return undefined
}

function rowClass(lineNumber: number): string {
  return [
    model.state.pipelineForm.hasHeaderRow && lineNumber === displayHeaderLine.value
      ? "preview-table-row-header"
      : "",
    lineNumber === displayDataStartLine.value ? "preview-table-row-data" : "",
  ]
    .filter(Boolean)
    .join(" ")
}

function lineButtonClass(lineNumber: number): string {
  return [
    "preview-line-button",
    model.state.pipelineForm.hasHeaderRow && lineNumber === displayHeaderLine.value
      ? "preview-line-button-header"
      : "",
    lineNumber === displayDataStartLine.value ? "preview-line-button-data" : "",
  ]
    .filter(Boolean)
    .join(" ")
}

function rowHandleClass(target: PreviewRowSelectionTarget): string {
  const active =
    model.state.pipelineSelectionTarget === target || rowDrag.value?.target === target
  const dragging = rowDrag.value?.target === target

  if (target === "header-row") {
    return [
      "preview-row-handle preview-row-handle-header",
      active ? "preview-row-handle-active" : "",
      dragging ? "preview-row-handle-dragging" : "",
    ]
      .filter(Boolean)
      .join(" ")
  }

  return [
    "preview-row-handle preview-row-handle-data",
    active ? "preview-row-handle-active" : "",
    dragging ? "preview-row-handle-dragging" : "",
  ]
    .filter(Boolean)
    .join(" ")
}

function timestampHandleClass(columnName: string): string {
  const active =
    displayTimestampColumn.value === columnName &&
    (model.state.pipelineSelectionTarget === "timestamp-column" || columnDrag.value !== null)
  const dragging = columnDrag.value?.originColumnName === columnName

  return [
    "preview-column-handle",
    active ? "preview-column-handle-active" : "",
    dragging ? "preview-column-handle-dragging" : "",
  ].join(" ")
}

function headerButtonClass(columnName: string): string {
  return [
    "preview-header-button",
  ]
    .filter(Boolean)
    .join(" ")
}

function cellClass(columnName: string): string {
  return [
    "preview-cell",
    model.previewColumnClass(columnName),
  ]
    .filter(Boolean)
    .join(" ")
}

function collectRowButtons() {
  return Array.from(
    rootRef.value?.querySelectorAll<HTMLButtonElement>("[data-preview-line-button]") ?? []
  )
    .map((button) => {
      const lineNumber = Number(button.dataset.previewLine)
      return Number.isFinite(lineNumber) ? { lineNumber, button } : null
    })
    .filter((entry): entry is { lineNumber: number; button: HTMLButtonElement } => entry !== null)
}

function collectHeaderButtons() {
  return Array.from(
    rootRef.value?.querySelectorAll<HTMLButtonElement>("[data-preview-column-button]") ?? []
  )
    .map((button) => {
      const columnName = button.dataset.previewColumn ?? ""
      return columnName ? { columnName, button } : null
    })
    .filter((entry): entry is { columnName: string; button: HTMLButtonElement } => entry !== null)
}

function nearestLineNumber(clientY: number): number | null {
  const buttons = collectRowButtons()
  if (buttons.length === 0) {
    return null
  }

  let bestLine = buttons[0].lineNumber
  let bestDistance = Number.POSITIVE_INFINITY

  for (const entry of buttons) {
    const rect = entry.button.getBoundingClientRect()
    const centerY = rect.top + rect.height / 2
    const distance = Math.abs(clientY - centerY)
    if (distance < bestDistance) {
      bestDistance = distance
      bestLine = entry.lineNumber
    }
  }

  return bestLine
}

function nearestColumnName(clientX: number): string | null {
  const buttons = collectHeaderButtons()
  if (buttons.length === 0) {
    return null
  }

  let bestColumn = buttons[0].columnName
  let bestDistance = Number.POSITIVE_INFINITY

  for (const entry of buttons) {
    const rect = entry.button.getBoundingClientRect()
    const centerX = rect.left + rect.width / 2
    const distance = Math.abs(clientX - centerX)
    if (distance < bestDistance) {
      bestDistance = distance
      bestColumn = entry.columnName
    }
  }

  return bestColumn
}

function onHandlePointerDown(
  target: PreviewRowSelectionTarget,
  lineNumber: number,
  event: PointerEvent
) {
  rowDrag.value = {
    target,
    originLineNumber: lineNumber,
    lineNumber,
    pointerId: event.pointerId,
    startClientY: event.clientY,
    currentClientY: event.clientY,
    moved: false,
  }
  model.state.pipelineSelectionTarget = target
  suppressHandleClick.value = false
  ;(event.currentTarget as HTMLElement | null)?.setPointerCapture?.(event.pointerId)
}

function onColumnHandlePointerDown(columnName: string, event: PointerEvent) {
  columnDrag.value = {
    originColumnName: columnName,
    columnName,
    pointerId: event.pointerId,
    startClientX: event.clientX,
    currentClientX: event.clientX,
    moved: false,
  }
  model.state.pipelineSelectionTarget = "timestamp-column"
  ;(event.currentTarget as HTMLElement | null)?.setPointerCapture?.(event.pointerId)
}

function onHandleClick(target: PreviewRowSelectionTarget) {
  if (suppressHandleClick.value) {
    suppressHandleClick.value = false
    return
  }
  model.state.pipelineSelectionTarget = target
}

function onWindowPointerMove(event: PointerEvent) {
  if (rowDrag.value?.pointerId === event.pointerId) {
    rowDrag.value.currentClientY = event.clientY
    const lineNumber = nearestLineNumber(event.clientY)
    if (lineNumber && lineNumber !== rowDrag.value.lineNumber) {
      rowDrag.value.lineNumber = lineNumber
      rowDrag.value.moved = true
    }
    return
  }

  if (columnDrag.value?.pointerId === event.pointerId) {
    columnDrag.value.currentClientX = event.clientX
    const columnName = nearestColumnName(event.clientX)
    if (columnName && columnName !== columnDrag.value.columnName) {
      columnDrag.value.columnName = columnName
      columnDrag.value.moved = true
    }
  }
}

function onWindowPointerUp(event: PointerEvent) {
  if (rowDrag.value?.pointerId === event.pointerId) {
    const drag = rowDrag.value
    rowDrag.value = null

    if (drag.moved) {
      if (drag.target === "header-row") {
        model.updateHeaderRowFromPreview(drag.lineNumber)
      } else {
        model.updateDataStartRowFromPreview(drag.lineNumber)
      }
      model.state.pipelineSelectionTarget = null
      suppressHandleClick.value = true
    } else {
      model.state.pipelineSelectionTarget = drag.target
    }
    return
  }

  if (columnDrag.value?.pointerId === event.pointerId) {
    const drag = columnDrag.value
    columnDrag.value = null

    if (drag.moved) {
      model.applyPreviewColumnSelection(drag.columnName)
    } else {
      model.state.pipelineSelectionTarget = "timestamp-column"
    }
  }
}

function onWindowPointerCancel(event: PointerEvent) {
  if (rowDrag.value?.pointerId === event.pointerId) {
    rowDrag.value = null
    suppressHandleClick.value = false
  }

  if (columnDrag.value?.pointerId === event.pointerId) {
    columnDrag.value = null
  }
}

onMounted(() => {
  window.addEventListener("pointermove", onWindowPointerMove)
  window.addEventListener("pointerup", onWindowPointerUp)
  window.addEventListener("pointercancel", onWindowPointerCancel)
})

onBeforeUnmount(() => {
  window.removeEventListener("pointermove", onWindowPointerMove)
  window.removeEventListener("pointerup", onWindowPointerUp)
  window.removeEventListener("pointercancel", onWindowPointerCancel)
})
</script>

<template>
  <article
    v-if="model.state.pipelinePreview"
    ref="rootRef"
    class="preview-card"
  >
    <div class="preview-header">
      <div>
        <p class="eyebrow">Preview</p>
        <h2 class="section-title">{{ previewFileName }}</h2>
        <p class="preview-guidance">{{ model.previewGuidanceText() }}</p>
      </div>
    </div>

    <label class="preview-toggle">
      <input
        class="preview-toggle-input"
        type="checkbox"
        :checked="model.state.pipelineForm.hasHeaderRow"
        @change="model.setPipelineHasHeaderRow(($event.target as HTMLInputElement).checked)"
      />
      <span class="preview-toggle-label">File has a header row</span>
    </label>

    <div class="preview-table-shell">
      <table class="preview-table">
        <thead>
          <tr>
            <th class="preview-cell preview-cell-line-number">Line</th>
            <th
              v-for="header in headers"
              :key="header"
              class="preview-cell"
              :class="model.previewColumnClass(header)"
            >
              <div class="preview-column-header">
                <button
                  v-if="displayTimestampColumn === header"
                  :class="timestampHandleClass(header)"
                  :style="timestampHandleStyle(header)"
                  type="button"
                  @click.prevent="model.state.pipelineSelectionTarget = 'timestamp-column'"
                  @pointerdown.prevent="onColumnHandlePointerDown(header, $event)"
                >
                  TIMESTAMP
                </button>
                <button
                  :class="headerButtonClass(header)"
                  type="button"
                  :data-preview-column="header"
                  data-preview-column-button
                  @click="model.applyPreviewColumnSelection(header)"
                >
                  {{ header }}
                </button>
              </div>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="entry in rows" :key="entry.lineNumber" :class="rowClass(entry.lineNumber)">
            <td class="preview-cell preview-cell-line-number preview-line-cell">
              <div class="preview-line-controls">
                <button
                  v-if="model.state.pipelineForm.hasHeaderRow && model.previewHandleLine('header-row') === entry.lineNumber"
                  :class="rowHandleClass('header-row')"
                  :style="handleOffsetStyle('header-row', entry.lineNumber)"
                  type="button"
                  @click.prevent="onHandleClick('header-row')"
                  @pointerdown.prevent="onHandlePointerDown('header-row', entry.lineNumber, $event)"
                >
                  HEADER
                </button>
                <button
                  v-if="model.previewHandleLine('data-start-row') === entry.lineNumber"
                  :class="rowHandleClass('data-start-row')"
                  :style="handleOffsetStyle('data-start-row', entry.lineNumber)"
                  type="button"
                  @click.prevent="onHandleClick('data-start-row')"
                  @pointerdown.prevent="onHandlePointerDown('data-start-row', entry.lineNumber, $event)"
                >
                  DATA START
                </button>
                <button
                  :class="lineButtonClass(entry.lineNumber)"
                  type="button"
                  :data-preview-line="entry.lineNumber"
                  data-preview-line-button
                  @click="model.applyPreviewLineSelection(entry.lineNumber)"
                >
                  <span class="preview-line-number">{{ entry.lineNumber }}</span>
                </button>
              </div>
            </td>
            <td
              v-for="(header, index) in headers"
              :key="`${entry.lineNumber}-${header}`"
              :class="cellClass(header)"
            >
              {{ entry.row[index] ?? "" }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <footer class="preview-footer">
      <span>
        Showing the first {{ shownLines }} lines of {{ model.state.pipelinePreview.total_lines }}
      </span>
      <button
        v-if="model.canShowMorePreviewLines()"
        class="btn-ghost preview-more-button"
        type="button"
        @click="model.showMorePreviewLines()"
      >
        Show {{ nextPageSize }} more line{{ nextPageSize === 1 ? "" : "s" }}
      </button>
    </footer>
  </article>

  <article
    v-else
    class="preview-card"
  >
    <div class="preview-placeholder">
      <div class="empty-icon">CSV</div>
      <h2 class="section-title">Preview a source file</h2>
      <p class="section-copy">
        Choose a CSV file path, then load the preview to inspect the first 50 lines and map the
        source structure into HydroServer.
      </p>
    </div>
  </article>
</template>
