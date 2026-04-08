import json
import tempfile
import unittest
from pathlib import Path

from sidecar.api.models import AppConfig, FileConfig, JobConfig
from sidecar.core.config import ConfigStore


class FileConfigModelTests(unittest.TestCase):
    def test_index_mode_clears_header_row_for_hydroserverpy(self) -> None:
        config = FileConfig.model_validate(
            {
                "headerRow": 1,
                "dataStartRow": 2,
                "delimiter": ",",
                "identifierType": "index",
                "timestamp": {
                    "key": "1",
                    "format": "ISO8601",
                    "timezoneMode": "embeddedOffset",
                },
            }
        )

        self.assertIsNone(config.header_row)
        self.assertEqual(
            config.model_dump(mode="json", by_alias=True),
            {
                "headerRow": None,
                "dataStartRow": 2,
                "delimiter": ",",
                "identifierType": "index",
                "timestamp": {
                    "key": "1",
                    "format": "ISO8601",
                    "customFormat": None,
                    "timezoneMode": "embeddedOffset",
                    "timezone": None,
                },
            },
        )

    def test_legacy_file_config_is_migrated_to_transformer_settings(self) -> None:
        config = FileConfig.model_validate(
            {
                "header_row": 3,
                "data_start_row": 4,
                "delimiter": ",",
                "timestamp_column": "Timestamp",
                "timestamp_format": "%Y-%m-%d %H:%M:%S",
                "timezone": "America/Denver",
            }
        )

        self.assertEqual(config.header_row, 3)
        self.assertEqual(config.data_start_row, 4)
        self.assertEqual(config.identifier_type, "name")
        self.assertEqual(config.timestamp.key, "Timestamp")
        self.assertEqual(config.timestamp.format, "custom")
        self.assertEqual(config.timestamp.custom_format, "%Y-%m-%d %H:%M:%S")
        self.assertEqual(config.timestamp.timezone_mode, "daylightSavings")
        self.assertEqual(config.timestamp.timezone, "America/Denver")

    def test_config_store_writes_camel_case_transformer_keys(self) -> None:
        file_config = FileConfig.model_validate(
            {
                "headerRow": 1,
                "dataStartRow": 2,
                "delimiter": "|",
                "identifierType": "name",
                "timestamp": {
                    "key": "recorded_at",
                    "format": "custom",
                    "customFormat": "%m/%d/%Y %H:%M:%S",
                    "timezoneMode": "daylightSavings",
                    "timezone": "America/Denver",
                },
            }
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            store = ConfigStore(Path(tmpdir))
            store.save(
                AppConfig(
                    jobs=[
                        JobConfig(
                            id="job-1",
                            name="Test Job",
                            file_path="/tmp/example.csv",
                            schedule_minutes=15,
                            file_config=file_config,
                            column_mappings=[],
                        )
                    ]
                )
            )

            payload = json.loads(Path(tmpdir, "config.json").read_text(encoding="utf-8"))
            saved_config = payload["jobs"][0]["file_config"]

            self.assertIn("headerRow", saved_config)
            self.assertIn("dataStartRow", saved_config)
            self.assertIn("identifierType", saved_config)
            self.assertEqual(saved_config["timestamp"]["customFormat"], "%m/%d/%Y %H:%M:%S")
            self.assertEqual(saved_config["timestamp"]["timezoneMode"], "daylightSavings")


if __name__ == "__main__":
    unittest.main()
