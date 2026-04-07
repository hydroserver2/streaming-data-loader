<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"

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

type RowEntry = { lineNumber: number; button: HTMLButtonElement }
type ColEntry = { columnName: string; button: HTMLButtonElement }

const model = useAppModel()

const rootRef = ref<HTMLElement | null>(null)
const rowDrag = ref<RowDragState | null>(null)
const columnDrag = ref<ColumnDragState | null>(null)
const suppressHandleClick = ref(false)

// Cached button collections — rebuilt when headers/rows change, not on every pointermove.
const cachedRowButtons = ref<RowEntry[]>([])
const cachedHeaderButtons = ref<ColEntry[]>([])

const headers = computed(() => model.previewHeaders.value)
const rows = computed(() =>
  model.parsedPreviewRows.value.map((row, index) => ({ lineNumber: index + 1, row }))
)
const shownLines = computed(() => model.state.pipelinePreview?.raw_lines.length ?? 0)
const nextPageSize = computed(() => {
  if (!model.state.pipelinePreview) return PREVIEW_PAGE_SIZE
  const remaining = Math.max(model.state.pipelinePreview.total_lines - shownLines.value, 0)
  return Math.min(PREVIEW_PAGE_SIZE, remaining)
})
const displayHeaderLine = computed(() =>
  rowDrag.value?.target === "header-row"
    ? rowDrag.value.lineNumber
    : model.state.pipelineForm.hasHeaderRow
      ? model.state.pipelineForm.headerRow
      : null
)
const displayDataStartLine = computed(() =>
  rowDrag.value?.target === "data-start-row"
    ? rowDrag.value.lineNumber
    : model.state.pipelineForm.dataStartRow
)
const displayTimestampColumn = computed(
  () => columnDrag.value?.columnName ?? model.state.pipelineForm.timestampColumn
)
const previewFileName = computed(
  () => model.state.pipelineForm.filePath.split(/[\\/]/).filter(Boolean).at(-1) ?? ""
)

// Rebuild caches after each render when rows/headers change.
function rebuildButtonCaches(): void {
  if (!rootRef.value) return

  cachedRowButtons.value = Array.from(
    rootRef.value.querySelectorAll<HTMLButtonElement>("[data-preview-line-button]")
  )
    .map((button) => {
      const lineNumber = Number(button.dataset.previewLine)
      return Number.isFinite(lineNumber) ? { lineNumber, button } : null
    })
    .filter((e): e is RowEntry => e !== null)

  cachedHeaderButtons.value = Array.from(
    rootRef.value.querySelectorAll<HTMLButtonElement>("[data-preview-column-button]")
  )
    .map((button) => {
      const columnName = button.dataset.previewColumn ?? ""
      return columnName ? { columnName, button } : null
    })
    .filter((e): e is ColEntry => e !== null)
}

// Watch for data changes that would add or remove buttons.
watch([rows, headers], () => {
  // nextTick not needed — Vue updates DOM synchronously before the watcher
  // callback fires when triggered from a template re-render. But we schedule
  // after the current microtask to be safe.
  Promise.resolve().then(rebuildButtonCaches)
})

// ── Drag helpers ───────────────────────────────────────────────────────────
function nearestLineNumber(clientY: number): number | null {
  const buttons = cachedRowButtons.value
  if (buttons.length === 0) return null
  return buttons.reduce(
    (best, entry) => {
      const rect = entry.button.getBoundingClientRect()
      const dist = Math.abs(clientY - (rect.top + rect.height / 2))
      return dist < best.dist ? { lineNumber: entry.lineNumber, dist } : best
    },
    { lineNumber: buttons[0].lineNumber, dist: Number.POSITIVE_INFINITY }
  ).lineNumber
}

function nearestColumnName(clientX: number): string | null {
  const buttons = cachedHeaderButtons.value
  if (buttons.length === 0) return null
  return buttons.reduce(
    (best, entry) => {
      const rect = entry.button.getBoundingClientRect()
      const dist = Math.abs(clientX - (rect.left + rect.width / 2))
      return dist < best.dist ? { columnName: entry.columnName, dist } : best
    },
    { columnName: buttons[0].columnName, dist: Number.POSITIVE_INFINITY }
  ).columnName
}

// ── Class helpers ──────────────────────────────────────────────────────────
function handleOffsetStyle(target: PreviewRowSelectionTarget, lineNumber: number) {
  if (
    rowDrag.value?.target === target &&
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
  if (columnDrag.value?.originColumnName === columnName) {
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
      ? "preview-table-row-header" : "",
    lineNumber === displayDataStartLine.value ? "preview-table-row-data" : "",
  ].filter(Boolean).join(" ")
}

function lineButtonClass(lineNumber: number): string {
  return [
    "preview-line-button",
    model.state.pipelineForm.hasHeaderRow && lineNumber === displayHeaderLine.value
      ? "preview-line-button-header" : "",
    lineNumber === displayDataStartLine.value ? "preview-line-button-data" : "",
  ].filter(Boolean).join(" ")
}

function rowHandleClass(target: PreviewRowSelectionTarget): string {
  const dragging = rowDrag.value?.target === target
  const base =
    target === "header-row"
      ? "preview-row-handle preview-row-handle-header"
      : "preview-row-handle preview-row-handle-data"
  return [base, dragging && "preview-row-handle-dragging"]
    .filter(Boolean).join(" ")
}

function timestampHandleClass(columnName: string): string {
  const dragging = columnDrag.value?.originColumnName === columnName
  return [
    "preview-column-handle",
    dragging && "preview-column-handle-dragging",
  ].filter(Boolean).join(" ")
}

function cellClass(columnName: string): string {
  const isTimestamp = columnName === model.state.pipelineForm.timestampColumn
  const isMapped = model.state.pipelineForm.mappings.some(
    (mapping) => mapping.csvColumn === columnName && mapping.datastreamId
  )
  return [
    "preview-cell",
    isTimestamp && "preview-col-timestamp",
    !isTimestamp && isMapped && "preview-col-mapped",
  ].filter(Boolean).join(" ")
}

// ── Pointer event handlers ─────────────────────────────────────────────────
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
  if (suppressHandleClick.value) { suppressHandleClick.value = false; return }
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
      if (drag.target === "header-row") model.updateHeaderRowFromPreview(drag.lineNumber)
      else model.updateDataStartRowFromPreview(drag.lineNumber)
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
    if (drag.moved) model.applyPreviewColumnSelection(drag.columnName)
    else model.state.pipelineSelectionTarget = "timestamp-column"
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
  rebuildButtonCaches()
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
        <p class="preview-guidance">
          Use this preview to set the header row, the first data row, and the
          timestamp column.
        </p>
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
              :class="cellClass(header)"
            >
              <div class="preview-column-header">
                <button
                  v-if="displayTimestampColumn === header"
                  :class="timestampHandleClass(header)"
                  :style="timestampHandleStyle(header)"
                  type="button"
                  @click.prevent
                  @pointerdown.prevent="onColumnHandlePointerDown(header, $event)"
                >
                  TIMESTAMP
                </button>
                <button
                  class="preview-header-button"
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
          <tr
            v-for="entry in rows"
            :key="entry.lineNumber"
            class="preview-table-row"
            :class="rowClass(entry.lineNumber)"
          >
            <td class="preview-cell preview-cell-line-number preview-line-cell">
              <div class="preview-line-controls">
                <button
                  v-if="model.state.pipelineForm.hasHeaderRow && displayHeaderLine === entry.lineNumber"
                  :class="rowHandleClass('header-row')"
                  :style="handleOffsetStyle('header-row', entry.lineNumber)"
                  type="button"
                  @click.prevent="onHandleClick('header-row')"
                  @pointerdown.prevent="onHandlePointerDown('header-row', entry.lineNumber, $event)"
                >
                  HEADER
                </button>
                <button
                  v-if="displayDataStartLine === entry.lineNumber"
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
      <span>Showing {{ shownLines }} of {{ model.state.pipelinePreview.total_lines }} lines</span>
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

  <article v-else class="preview-card">
    <div class="preview-placeholder">
      <div class="empty-icon">CSV</div>
      <h2 class="section-title">Preview a source file</h2>
      <p class="section-copy">
        Choose a CSV file path to inspect the first 50 lines and configure the source structure.
      </p>
    </div>
  </article>
</template>
