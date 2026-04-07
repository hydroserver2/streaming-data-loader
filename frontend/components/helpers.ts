export type Feedback = {
  tone: "success" | "error" | "info";
  message: string;
} | null;

export const APP_NAME = "HydroServer Streaming Data Loader";

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function feedbackMarkup(feedback: Feedback): string {
  if (!feedback) return "";
  const cls =
    feedback.tone === "success"
      ? "notice-success"
      : feedback.tone === "error"
      ? "notice-error"
      : "notice-info";
  return `<div class="${cls}">${escapeHtml(feedback.message)}</div>`;
}

export function basename(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments.at(-1) ?? path;
}

export function parseDelimitedLine(line: string, delimiter: string): string[] {
  if (!delimiter) return [line];
  const cells: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '"') {
      if (inQuotes && line[i + 1] === '"') {
        current += '"';
        i++;
      } else {
        inQuotes = !inQuotes;
      }
      continue;
    }
    if (!inQuotes && line.startsWith(delimiter, i)) {
      cells.push(current);
      current = "";
      i += delimiter.length - 1;
      continue;
    }
    current += char;
  }
  cells.push(current);
  return cells;
}
