from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from sidecar.core.loader import _detect_delimiter, preview_csv


class CsvPreviewTests(unittest.TestCase):
    def test_detect_delimiter_supports_pipe_files(self) -> None:
        delimiter = _detect_delimiter(
            [
                "timestamp|value|quality",
                "2024-01-01T00:00:00Z|1.2|good",
                "2024-01-02T00:00:00Z|1.5|good",
            ]
        )

        self.assertEqual(delimiter, "|")

    def test_detect_delimiter_supports_space_delimited_files(self) -> None:
        delimiter = _detect_delimiter(
            [
                "date value quality",
                "2024-01-01 1.2 good",
                "2024-01-02 1.5 good",
            ]
        )

        self.assertEqual(delimiter, " ")

    def test_preview_csv_reports_detected_pipe_delimiter(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            csv_path = Path(temp_dir) / "pipe-preview.csv"
            csv_path.write_text(
                "\n".join(
                    [
                        "timestamp|value|quality",
                        "2024-01-01T00:00:00|1.2|2.4",
                        "2024-01-02T00:00:00|1.5|2.7",
                    ]
                ),
                encoding="utf-8",
            )

            preview = preview_csv(str(csv_path))

        self.assertEqual(preview.detected_delimiter, "|")
        self.assertEqual(preview.detected_header_row, 1)
        self.assertEqual(preview.detected_data_start_row, 2)


if __name__ == "__main__":
    unittest.main()
