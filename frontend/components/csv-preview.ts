import {
  state,
  previewHeaders,
  parsedPreviewRows,
  previewHandleLine,
  activePreviewRowTarget,
  activeTimestampColumn,
  previewCommittedHandleLine,
  canShowMorePreviewLines,
  updateHeaderRowFromPreview,
  updateDataStartRowFromPreview,
  initializeMappings,
  PREVIEW_PAGE_SIZE,
  type PreviewRowSelectionTarget,
} from "../state";
import { escapeHtml, basename } from "./helpers";

// ── Module-level drag visual state ─────────────────────────────────────────
// These track live DOM positions during a drag gesture. They are not render
// state and must not be stored in UiState.

type PreviewDragVisualState = {
  handle: HTMLElement;
  startClientY: number;
  currentClientY: number;
  rowButtons: Map<number, HTMLButtonElement>;
  rowElements: Map<number, HTMLTableRowElement>;
  rowCenters: Array<{ lineNumber: number; centerY: number }>;
  frameRequested: boolean;
};

type PreviewColumnDragVisualState = {
  handle: HTMLElement;
  startClientX: number;
  currentClientX: number;
  headerButtons: Map<string, HTMLButtonElement>;
  columnCells: Map<string, HTMLElement[]>;
  headerCenters: Array<{ columnName: string; centerX: number }>;
  frameRequested: boolean;
};

let _suppressHandleClick = false;
let _rowDragVisual: PreviewDragVisualState | null = null;
let _colDragVisual: PreviewColumnDragVisualState | null = null;

export function getSuppressHandleClick(): boolean {
  return _suppressHandleClick;
}
export function clearSuppressHandleClick(): void {
  _suppressHandleClick = false;
}

// ── Rendering helpers ──────────────────────────────────────────────────────
export function previewColumnClass(columnName: string): string {
  if (columnName === state.pipelineForm.timestampColumn) return "preview-col-timestamp";
  const mapped = state.pipelineForm.mappings.find(
    (m) => m.csvColumn === columnName && m.datastreamId
  );
  return mapped ? "preview-col-mapped" : "";
}

export function previewFieldClass(
  target: Exclude<typeof state.pipelineSelectionTarget, null>
): string {
  const active =
    target === "timestamp-column"
      ? state.pipelineSelectionTarget === target || state.pipelineColumnDrag !== null
      : activePreviewRowTarget() === target;
  const toneClass =
    target === "header-row"
      ? "preview-bound-field-header"
      : target === "data-start-row"
      ? "preview-bound-field-data"
      : "preview-bound-field-timestamp";
  return active
    ? `field preview-bound-field preview-bound-field-active ${toneClass}`
    : "field preview-bound-field";
}

function previewGuidanceText(): string {
  const activeTarget = activePreviewRowTarget();
  if (activeTarget === "header-row") {
    return "Drag the HEADER handle, or click a row to place it.";
  }
  if (activeTarget === "data-start-row") {
    return "Drag the DATA START handle, or click the first data row.";
  }
  if (state.pipelineSelectionTarget === "timestamp-column" || state.pipelineColumnDrag) {
    return "Drag the TIMESTAMP handle, or click a column header to place it.";
  }
  return state.pipelineForm.hasHeaderRow
    ? "Drag the HEADER, DATA START, and TIMESTAMP handles, or click a row or column to place them."
    : "Drag the DATA START and TIMESTAMP handles, or click a row or column to place them.";
}

function renderPreviewHandle(
  target: PreviewRowSelectionTarget,
  lineNumber: number
): string {
  if (previewHandleLine(target) !== lineNumber) return "";
  const active = activePreviewRowTarget() === target;
  const label = target === "header-row" ? "HEADER" : "DATA START";
  const base =
    target === "header-row"
      ? "preview-row-handle preview-row-handle-header"
      : "preview-row-handle preview-row-handle-data";
  return `
    <button
      class="${active ? `${base} preview-row-handle-active` : base}"
      type="button"
      data-action="activate-preview-handle"
      data-preview-handle-target="${target}"
      data-preview-line="${lineNumber}"
    >${label}</button>
  `;
}

function renderTimestampHandle(columnName: string): string {
  if (columnName !== activeTimestampColumn()) return "";
  const active =
    state.pipelineSelectionTarget === "timestamp-column" ||
    state.pipelineColumnDrag !== null;
  return `
    <button
      class="${active ? "preview-column-handle preview-column-handle-active" : "preview-column-handle"}"
      type="button"
      data-preview-column-handle="${escapeHtml(columnName)}"
    >TIMESTAMP</button>
  `;
}

// ── Public render function ─────────────────────────────────────────────────
export function renderPipelinePreview(): string {
  if (!state.pipelinePreview) {
    return `
      <article class="preview-card preview-placeholder">
        <div class="empty-icon">CSV</div>
        <h2 class="section-title">Preview a source file</h2>
        <p class="section-copy">
          Choose a CSV file to inspect the first 50 lines and configure
          the source structure.
        </p>
      </article>
    `;
  }

  const headers = previewHeaders();
  const parsedRows = parsedPreviewRows().map((row, i) => ({
    lineNumber: i + 1,
    row,
  }));
  const headerLine = previewHandleLine("header-row");
  const dataStartLine = previewHandleLine("data-start-row");

  const headerCells = headers
    .map(
      (header) => `
        <th
          class="preview-cell ${previewColumnClass(header)}"
          data-preview-column-cell="${escapeHtml(header)}"
        >
          <div class="preview-column-header">
            ${renderTimestampHandle(header)}
            <button
              class="preview-header-button"
              type="button"
              data-action="pick-preview-column"
              data-preview-column="${escapeHtml(header)}"
            >${escapeHtml(header)}</button>
          </div>
        </th>
      `
    )
    .join("");

  const tableRows = parsedRows
    .map(({ lineNumber, row }) => {
      const rowClasses = [
        "preview-table-row",
        state.pipelineForm.hasHeaderRow && lineNumber === headerLine
          ? "preview-table-row-header"
          : "",
        lineNumber === dataStartLine ? "preview-table-row-data" : "",
      ]
        .filter(Boolean)
        .join(" ");

      const dataCells = headers
        .map(
          (columnName, i) => `
            <td
              class="preview-cell ${previewColumnClass(columnName)}"
              data-preview-column-cell="${escapeHtml(columnName)}"
            >${escapeHtml(row[i] ?? "")}</td>
          `
        )
        .join("");

      return `
        <tr class="${rowClasses}" data-preview-line-row="${lineNumber}">
          <td class="preview-cell preview-cell-line-number preview-line-cell">
            <div class="preview-line-controls">
              ${state.pipelineForm.hasHeaderRow ? renderPreviewHandle("header-row", lineNumber) : ""}
              ${renderPreviewHandle("data-start-row", lineNumber)}
              <button
                class="preview-line-button"
                type="button"
                data-action="pick-preview-line"
                data-preview-line="${lineNumber}"
              ><span class="preview-line-number">${lineNumber}</span></button>
            </div>
          </td>
          ${dataCells}
        </tr>
      `;
    })
    .join("");

  const shownLines = state.pipelinePreview.raw_lines.length;
  const remaining = Math.max(state.pipelinePreview.total_lines - shownLines, 0);
  const nextPageSize = Math.min(PREVIEW_PAGE_SIZE, remaining);
  const showMoreButton = canShowMorePreviewLines()
    ? `<button class="btn-ghost preview-more-button" type="button" data-action="show-more-preview-lines">
        Show ${nextPageSize} more line${nextPageSize === 1 ? "" : "s"}
      </button>`
    : "";

  return `
    <article class="preview-card">
      <header class="preview-header">
        <p class="eyebrow">Preview</p>
        <h2 class="section-title">${escapeHtml(basename(state.pipelineForm.filePath))}</h2>
        <p class="preview-guidance">${escapeHtml(previewGuidanceText())}</p>
      </header>

      <label class="preview-toggle">
        <input
          class="preview-toggle-input"
          type="checkbox"
          name="has_header_row"
          ${state.pipelineForm.hasHeaderRow ? "checked" : ""}
        />
        <span class="preview-toggle-label">File has a header row</span>
      </label>

      <div class="preview-table-shell">
        <table class="preview-table">
          <thead>
            <tr>
              <th class="preview-cell preview-cell-line-number">Line</th>
              ${headerCells}
            </tr>
          </thead>
          <tbody>${tableRows}</tbody>
        </table>
      </div>

      <footer class="preview-footer">
        <span>Showing ${shownLines} of ${state.pipelinePreview.total_lines} lines</span>
        ${showMoreButton}
      </footer>
    </article>
  `;
}

// ── DOM collection helpers ─────────────────────────────────────────────────
function collectRowButtons(
  root: HTMLElement
): Map<number, HTMLButtonElement> {
  return new Map(
    Array.from(
      root.querySelectorAll<HTMLButtonElement>(
        '[data-action="pick-preview-line"][data-preview-line]'
      )
    )
      .map((btn) => {
        const n = Number(btn.dataset.previewLine);
        return Number.isFinite(n) ? ([n, btn] as [number, HTMLButtonElement]) : null;
      })
      .filter((e): e is [number, HTMLButtonElement] => e !== null)
  );
}

function collectRowElements(
  root: HTMLElement
): Map<number, HTMLTableRowElement> {
  return new Map(
    Array.from(root.querySelectorAll<HTMLTableRowElement>("[data-preview-line-row]"))
      .map((row) => {
        const n = Number(row.dataset.previewLineRow);
        return Number.isFinite(n)
          ? ([n, row] as [number, HTMLTableRowElement])
          : null;
      })
      .filter((e): e is [number, HTMLTableRowElement] => e !== null)
  );
}

function collectRowCenters(
  rowButtons: Map<number, HTMLButtonElement>
): Array<{ lineNumber: number; centerY: number }> {
  return Array.from(rowButtons.entries()).map(([lineNumber, btn]) => {
    const rect = btn.getBoundingClientRect();
    return { lineNumber, centerY: rect.top + rect.height / 2 };
  });
}

function nearestLineNumber(
  clientY: number,
  rowCenters: Array<{ lineNumber: number; centerY: number }>
): number | null {
  if (rowCenters.length === 0) return null;
  return rowCenters.reduce((best, entry) =>
    Math.abs(clientY - entry.centerY) < Math.abs(clientY - best.centerY)
      ? entry
      : best
  ).lineNumber;
}

function collectHeaderButtons(
  root: HTMLElement
): Map<string, HTMLButtonElement> {
  return new Map(
    Array.from(
      root.querySelectorAll<HTMLButtonElement>(
        '[data-action="pick-preview-column"][data-preview-column]'
      )
    )
      .map((btn) => {
        const col = btn.dataset.previewColumn ?? "";
        return col ? ([col, btn] as [string, HTMLButtonElement]) : null;
      })
      .filter((e): e is [string, HTMLButtonElement] => e !== null)
  );
}

function collectColumnCells(root: HTMLElement): Map<string, HTMLElement[]> {
  const cells = new Map<string, HTMLElement[]>();
  root.querySelectorAll<HTMLElement>("[data-preview-column-cell]").forEach((el) => {
    const col = el.dataset.previewColumnCell ?? "";
    if (!col) return;
    const bucket = cells.get(col) ?? [];
    bucket.push(el);
    cells.set(col, bucket);
  });
  return cells;
}

function collectHeaderCenters(
  headerButtons: Map<string, HTMLButtonElement>
): Array<{ columnName: string; centerX: number }> {
  return Array.from(headerButtons.entries()).map(([columnName, btn]) => {
    const rect = btn.getBoundingClientRect();
    return { columnName, centerX: rect.left + rect.width / 2 };
  });
}

function nearestColumnName(
  clientX: number,
  headerCenters: Array<{ columnName: string; centerX: number }>
): string | null {
  if (headerCenters.length === 0) return null;
  return headerCenters.reduce((best, entry) =>
    Math.abs(clientX - entry.centerX) < Math.abs(clientX - best.centerX)
      ? entry
      : best
  ).columnName;
}

// ── Row drag visual ────────────────────────────────────────────────────────
function applyRowDragClasses(): void {
  if (!state.pipelineDrag || !_rowDragVisual) return;

  const headerLine =
    state.pipelineDrag.target === "header-row"
      ? state.pipelineDrag.lineNumber
      : previewCommittedHandleLine("header-row");
  const dataLine =
    state.pipelineDrag.target === "data-start-row"
      ? state.pipelineDrag.lineNumber
      : previewCommittedHandleLine("data-start-row");

  for (const [n, btn] of _rowDragVisual.rowButtons.entries()) {
    btn.classList.toggle(
      "preview-line-button-header",
      state.pipelineForm.hasHeaderRow && headerLine === n
    );
    btn.classList.toggle("preview-line-button-data", dataLine === n);
  }
  for (const [n, row] of _rowDragVisual.rowElements.entries()) {
    row.classList.toggle(
      "preview-table-row-header",
      state.pipelineForm.hasHeaderRow && headerLine === n
    );
    row.classList.toggle("preview-table-row-data", dataLine === n);
  }
}

function flushRowDragVisual(): void {
  if (!state.pipelineDrag || !_rowDragVisual) return;
  _rowDragVisual.frameRequested = false;
  const offset = _rowDragVisual.currentClientY - _rowDragVisual.startClientY;
  _rowDragVisual.handle.style.setProperty("--preview-handle-offset", `${offset}px`);
  applyRowDragClasses();
}

function scheduleRowDragVisual(): void {
  if (!_rowDragVisual || _rowDragVisual.frameRequested) return;
  _rowDragVisual.frameRequested = true;
  window.requestAnimationFrame(flushRowDragVisual);
}

function beginRowDragVisual(root: HTMLElement, clientY: number): void {
  if (!state.pipelineDrag) return;
  const { target, lineNumber } = state.pipelineDrag;
  const handle = root.querySelector<HTMLElement>(
    `[data-preview-handle-target="${target}"][data-preview-line="${lineNumber}"]`
  );
  if (!handle) return;

  const rowButtons = collectRowButtons(root);
  _rowDragVisual = {
    handle,
    startClientY: clientY,
    currentClientY: clientY,
    rowButtons,
    rowElements: collectRowElements(root),
    rowCenters: collectRowCenters(rowButtons),
    frameRequested: false,
  };

  root
    .querySelectorAll<HTMLElement>(".preview-row-handle-active")
    .forEach((el) => el.classList.remove("preview-row-handle-active"));
  handle.classList.add("preview-row-handle-active", "preview-row-handle-dragging");
  handle.style.setProperty("--preview-handle-offset", "0px");
  applyRowDragClasses();
}

function endRowDragVisual(): void {
  if (!_rowDragVisual) return;
  if (
    state.pipelineDrag &&
    typeof _rowDragVisual.handle.releasePointerCapture === "function" &&
    _rowDragVisual.handle.hasPointerCapture(state.pipelineDrag.pointerId)
  ) {
    _rowDragVisual.handle.releasePointerCapture(state.pipelineDrag.pointerId);
  }
  _rowDragVisual.handle.classList.remove("preview-row-handle-dragging");
  _rowDragVisual.handle.style.removeProperty("--preview-handle-offset");
  _rowDragVisual = null;
}

// ── Column drag visual ─────────────────────────────────────────────────────
function applyColDragClasses(): void {
  if (!state.pipelineColumnDrag || !_colDragVisual) return;
  for (const [col, cells] of _colDragVisual.columnCells.entries()) {
    const active = col === state.pipelineColumnDrag.columnName;
    for (const cell of cells) cell.classList.toggle("preview-col-timestamp", active);
  }
}

function flushColDragVisual(): void {
  if (!state.pipelineColumnDrag || !_colDragVisual) return;
  _colDragVisual.frameRequested = false;
  const offset = _colDragVisual.currentClientX - _colDragVisual.startClientX;
  _colDragVisual.handle.style.setProperty(
    "--preview-column-handle-offset",
    `${offset}px`
  );
  applyColDragClasses();
}

function scheduleColDragVisual(): void {
  if (!_colDragVisual || _colDragVisual.frameRequested) return;
  _colDragVisual.frameRequested = true;
  window.requestAnimationFrame(flushColDragVisual);
}

function beginColDragVisual(root: HTMLElement, clientX: number): void {
  if (!state.pipelineColumnDrag) return;
  const { columnName } = state.pipelineColumnDrag;
  const handle =
    Array.from(root.querySelectorAll<HTMLElement>("[data-preview-column-handle]")).find(
      (el) => el.dataset.previewColumnHandle === columnName
    ) ?? null;
  if (!handle) return;

  const headerButtons = collectHeaderButtons(root);
  _colDragVisual = {
    handle,
    startClientX: clientX,
    currentClientX: clientX,
    headerButtons,
    columnCells: collectColumnCells(root),
    headerCenters: collectHeaderCenters(headerButtons),
    frameRequested: false,
  };

  handle.classList.add("preview-column-handle-dragging");
  handle.style.setProperty("--preview-column-handle-offset", "0px");
  applyColDragClasses();
}

function endColDragVisual(): void {
  if (!_colDragVisual) return;
  if (
    state.pipelineColumnDrag &&
    typeof _colDragVisual.handle.releasePointerCapture === "function" &&
    _colDragVisual.handle.hasPointerCapture(state.pipelineColumnDrag.pointerId)
  ) {
    _colDragVisual.handle.releasePointerCapture(state.pipelineColumnDrag.pointerId);
  }
  _colDragVisual.handle.classList.remove("preview-column-handle-dragging");
  _colDragVisual.handle.style.removeProperty("--preview-column-handle-offset");
  _colDragVisual = null;
}

// ── Public event initializer ───────────────────────────────────────────────
// Sets up all pointer events needed for drag-to-select on the preview table.
// Call once after the DOM shell is ready.
export function initPreviewDragEvents(
  mainContent: HTMLElement,
  render: () => void
): void {
  // ── Pointer down: start a drag ─────────────────────────────────────────
  mainContent.addEventListener("pointerdown", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;

    // Row handle drag (header-row or data-start-row)
    const handle = target.closest<HTMLElement>("[data-preview-handle-target]");
    if (handle) {
      const pickerTarget = handle.dataset.previewHandleTarget;
      if (pickerTarget !== "header-row" && pickerTarget !== "data-start-row") return;
      const lineNumber = Number(handle.dataset.previewLine);
      if (!Number.isFinite(lineNumber) || lineNumber < 1) return;

      state.pipelineSelectionTarget = pickerTarget;
      state.pipelineDrag = {
        target: pickerTarget,
        lineNumber,
        pointerId: event.pointerId,
        moved: false,
      };
      _suppressHandleClick = false;
      if (typeof handle.setPointerCapture === "function") {
        handle.setPointerCapture(event.pointerId);
      }
      beginRowDragVisual(mainContent, event.clientY);
      event.preventDefault();
      return;
    }

    // Column handle drag (timestamp-column)
    const colHandle = target.closest<HTMLElement>("[data-preview-column-handle]");
    if (!colHandle) return;
    const columnName = colHandle.dataset.previewColumnHandle ?? "";
    if (!columnName) return;

    state.pipelineSelectionTarget = "timestamp-column";
    state.pipelineColumnDrag = {
      columnName,
      pointerId: event.pointerId,
      moved: false,
    };
    if (typeof colHandle.setPointerCapture === "function") {
      colHandle.setPointerCapture(event.pointerId);
    }
    beginColDragVisual(mainContent, event.clientX);
    event.preventDefault();
  });

  // ── Pointer move: update drag position ────────────────────────────────
  window.addEventListener("pointermove", (event) => {
    if (state.pipelineDrag?.pointerId === event.pointerId) {
      if (!_rowDragVisual) return;
      _rowDragVisual.currentClientY = event.clientY;
      const lineNumber = nearestLineNumber(event.clientY, _rowDragVisual.rowCenters);
      if (lineNumber === null) {
        scheduleRowDragVisual();
        return;
      }
      if (lineNumber !== state.pipelineDrag.lineNumber) {
        state.pipelineDrag = { ...state.pipelineDrag, lineNumber, moved: true };
      }
      scheduleRowDragVisual();
      return;
    }

    if (
      !state.pipelineColumnDrag ||
      state.pipelineColumnDrag.pointerId !== event.pointerId
    ) {
      return;
    }
    if (!_colDragVisual) return;
    _colDragVisual.currentClientX = event.clientX;
    const columnName = nearestColumnName(event.clientX, _colDragVisual.headerCenters);
    if (columnName && columnName !== state.pipelineColumnDrag.columnName) {
      state.pipelineColumnDrag = { ...state.pipelineColumnDrag, columnName, moved: true };
    }
    scheduleColDragVisual();
  });

  // ── Pointer up: commit drag ────────────────────────────────────────────
  window.addEventListener("pointerup", (event) => {
    if (state.pipelineDrag?.pointerId === event.pointerId) {
      const drag = state.pipelineDrag;
      endRowDragVisual();
      state.pipelineDrag = null;

      if (drag.moved) {
        if (drag.target === "header-row") {
          updateHeaderRowFromPreview(drag.lineNumber);
        } else {
          updateDataStartRowFromPreview(drag.lineNumber);
        }
        state.pipelineSelectionTarget = null;
        _suppressHandleClick = true;
      } else {
        state.pipelineSelectionTarget = drag.target;
        _suppressHandleClick = false;
      }
      render();
      return;
    }

    if (
      !state.pipelineColumnDrag ||
      state.pipelineColumnDrag.pointerId !== event.pointerId
    ) {
      return;
    }
    const drag = state.pipelineColumnDrag;
    endColDragVisual();
    state.pipelineColumnDrag = null;

    if (drag.moved) {
      state.pipelineForm.timestampColumn = drag.columnName;
      initializeMappings(previewHeaders());
      state.pipelineSelectionTarget = null;
    } else {
      state.pipelineSelectionTarget = "timestamp-column";
    }
    render();
  });

  // ── Pointer cancel: abort drag ─────────────────────────────────────────
  window.addEventListener("pointercancel", (event) => {
    if (state.pipelineDrag?.pointerId === event.pointerId) {
      endRowDragVisual();
      state.pipelineDrag = null;
      _suppressHandleClick = false;
      render();
      return;
    }
    if (
      !state.pipelineColumnDrag ||
      state.pipelineColumnDrag.pointerId !== event.pointerId
    ) {
      return;
    }
    endColDragVisual();
    state.pipelineColumnDrag = null;
    state.pipelineSelectionTarget = null;
    render();
  });
}
