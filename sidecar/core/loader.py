from __future__ import annotations

import csv
from datetime import datetime
from pathlib import Path

from sidecar.api.models import CsvPreviewResponse


DELIMITER_CANDIDATES = [",", "\t", ";", "|", " "]
TIMESTAMP_FORMATS = [
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%m/%d/%Y %H:%M",
]


def preview_csv(path: str, rows: int = 100) -> CsvPreviewResponse:
    file_path = Path(path).expanduser()
    if not file_path.exists():
        raise FileNotFoundError("Can't find the data file. It may have been moved or renamed.")

    raw_text, encoding = _read_text(file_path)
    raw_lines = raw_text.splitlines()
    delimiter = _detect_delimiter(raw_lines[:rows])
    parsed_rows = [_parse_line(line, delimiter) for line in raw_lines[:rows] if line.strip()]
    header_index = _detect_header_row(parsed_rows)
    data_start_index = _detect_data_start_row(parsed_rows, header_index)

    return CsvPreviewResponse(
        raw_lines=raw_lines[:rows],
        parsed_rows=parsed_rows[header_index:] if header_index is not None else parsed_rows,
        detected_header_row=header_index + 1 if header_index is not None else None,
        detected_data_start_row=data_start_index + 1 if data_start_index is not None else None,
        detected_delimiter=delimiter,
        total_lines=len(raw_lines),
        encoding=encoding,
    )


def _read_text(path: Path) -> tuple[str, str]:
    for encoding in ("utf-8", "utf-8-sig", "latin-1"):
        try:
            return path.read_text(encoding=encoding), encoding
        except UnicodeDecodeError:
            continue
    raise UnicodeDecodeError("utf-8", b"", 0, 1, "Unsupported file encoding")


def _detect_delimiter(lines: list[str]) -> str:
    best_delimiter = ","
    best_score = -1

    for delimiter in DELIMITER_CANDIDATES:
        counts = [len(_parse_line(line, delimiter)) for line in lines if line.strip()]
        if not counts:
            continue
        mode_count = max(set(counts), key=counts.count)
        score = counts.count(mode_count) * mode_count
        if score > best_score:
            best_score = score
            best_delimiter = delimiter

    return best_delimiter


def _parse_line(line: str, delimiter: str) -> list[str]:
    return next(csv.reader([line], delimiter=delimiter))


def _detect_header_row(rows: list[list[str]]) -> int | None:
    for index, row in enumerate(rows):
        cleaned = [cell.strip() for cell in row if cell.strip()]
        if len(cleaned) < 3:
            continue
        if all(not _looks_numeric_or_timestamp(cell) for cell in cleaned):
            return index
    return 0 if rows else None


def _detect_data_start_row(rows: list[list[str]], header_index: int | None) -> int | None:
    if header_index is None:
        return None

    expected_columns = len(rows[header_index]) if rows else 0
    for index in range(header_index + 1, len(rows)):
        row = [cell.strip() for cell in rows[index]]
        if len(row) != expected_columns:
            continue
        meaningful = [cell for cell in row if cell]
        if len(meaningful) < 2:
            continue
        if sum(1 for cell in meaningful if _looks_numeric_or_timestamp(cell)) >= max(2, len(meaningful) // 2):
            return index
    return None


def _looks_numeric_or_timestamp(value: str) -> bool:
    try:
        float(value)
        return True
    except ValueError:
        pass

    for timestamp_format in TIMESTAMP_FORMATS:
        try:
            datetime.strptime(value, timestamp_format)
            return True
        except ValueError:
            continue

    return False
