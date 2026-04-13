import json
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

from sidecar.api.models import (
    AppConfig,
    AppStateFile,
    FileConfig,
    JobConfig,
    JobCursor,
    JobLogEntry,
    JobUpsertRequest,
    ServerConfig,
)
from sidecar.core.config import ConfigStore
from sidecar.core.state import StateStore


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


class WorkspacePersistenceTests(unittest.TestCase):
    def create_store(self, tmpdir: str) -> tuple[ConfigStore, StateStore]:
        config_store = ConfigStore(Path(tmpdir))
        return config_store, StateStore(config_store)

    def test_set_server_creates_workspace_file_and_uses_it_for_jobs(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            store, _ = self.create_store(tmpdir)
            config = store.set_server(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="secret",
                    username="",
                    password="",
                    workspace_id="workspace-123",
                ),
                workspace_name="Primary Workspace",
            )

            self.assertEqual(config.jobs, [])

            workspace_path = Path(tmpdir, "workspaces", "workspace-123.json")
            self.assertTrue(workspace_path.exists())

            payload = json.loads(workspace_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["workspace_id"], "workspace-123")
            self.assertEqual(payload["workspace_name"], "Primary Workspace")
            self.assertEqual(payload["hydroserver_url"], "https://example.com")
            self.assertEqual(payload["datasources"], [])

    def test_config_store_writes_camel_case_transformer_keys_to_workspace_files(self) -> None:
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
            store, _ = self.create_store(tmpdir)
            store.save(
                AppConfig(
                    server=ServerConfig(
                        auth_type="apikey",
                        url="https://example.com",
                        api_key="secret",
                        username="",
                        password="",
                        workspace_id="workspace-123",
                    ),
                    jobs=[
                        JobConfig(
                            id="job-1",
                            name="Test Job",
                            file_path="/tmp/example.csv",
                            schedule_minutes=15,
                            file_config=file_config,
                            column_mappings=[],
                        )
                    ],
                )
            )

            payload = json.loads(
                Path(tmpdir, "workspaces", "workspace-123.json").read_text(
                    encoding="utf-8"
                )
            )
            saved_config = payload["datasources"][0]["file_config"]

            self.assertIn("headerRow", saved_config)
            self.assertIn("dataStartRow", saved_config)
            self.assertIn("identifierType", saved_config)
            self.assertEqual(
                saved_config["timestamp"]["customFormat"], "%m/%d/%Y %H:%M:%S"
            )
            self.assertEqual(
                saved_config["timestamp"]["timezoneMode"], "daylightSavings"
            )

    def test_workspace_switches_isolate_datasource_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            store, _ = self.create_store(tmpdir)

            store.set_server(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="secret-a",
                    username="",
                    password="",
                    workspace_id="workspace-a",
                ),
                workspace_name="Workspace A",
            )
            job_a = store.create_job(
                JobUpsertRequest(
                    name="A Job",
                    file_path="/tmp/a.csv",
                    schedule_minutes=15,
                    file_config=FileConfig(),
                    column_mappings=[],
                )
            )

            store.set_server(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="secret-b",
                    username="",
                    password="",
                    workspace_id="workspace-b",
                ),
                workspace_name="Workspace B",
            )

            self.assertEqual(store.list_jobs(), [])
            store.create_job(
                JobUpsertRequest(
                    name="B Job",
                    file_path="/tmp/b.csv",
                    schedule_minutes=30,
                    file_config=FileConfig(),
                    column_mappings=[],
                )
            )

            store.set_server(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="secret-a",
                    username="",
                    password="",
                    workspace_id="workspace-a",
                ),
                workspace_name="Workspace A",
            )

            workspace_a_jobs = store.list_jobs()
            self.assertEqual(len(workspace_a_jobs), 1)
            self.assertEqual(workspace_a_jobs[0].name, "A Job")
            self.assertTrue(Path(tmpdir, "workspaces", "workspace-a.json").exists())
            self.assertTrue(Path(tmpdir, "workspaces", "workspace-b.json").exists())
            self.assertIsNotNone(job_a.id)

    def test_state_store_persists_cursor_and_logs_inside_workspace_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            store, state_store = self.create_store(tmpdir)
            store.set_server(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="secret",
                    username="",
                    password="",
                    workspace_id="workspace-123",
                )
            )
            job = store.create_job(
                JobUpsertRequest(
                    name="Test Job",
                    file_path="/tmp/example.csv",
                    schedule_minutes=15,
                    file_config=FileConfig(),
                    column_mappings=[],
                )
            )

            cursor = JobCursor(
                last_pushed_timestamp=datetime(2026, 4, 13, tzinfo=timezone.utc),
                last_pushed_row_index=42,
                last_run_at=datetime(2026, 4, 13, 1, tzinfo=timezone.utc),
                last_error=None,
            )

            state_store.update_cursor(job.id, cursor)
            state_store.append_log(job.id, "Job completed")

            payload = json.loads(
                Path(tmpdir, "workspaces", "workspace-123.json").read_text(
                    encoding="utf-8"
                )
            )
            datasource = payload["datasources"][0]
            self.assertEqual(datasource["last_pushed_row_index"], 42)
            self.assertEqual(datasource["recent_logs"][0]["message"], "Job completed")

    def test_legacy_global_jobs_and_state_are_migrated_to_workspace_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir, "config.json")
            state_path = Path(tmpdir, "state.json")

            config_path.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "server": {
                            "auth_type": "apikey",
                            "url": "https://example.com",
                            "api_key": "secret",
                            "username": "",
                            "password": "",
                            "workspace_id": "workspace-123",
                        },
                        "jobs": [
                            {
                                "id": "job-1",
                                "name": "Migrated Job",
                                "enabled": True,
                                "file_path": "/tmp/example.csv",
                                "schedule_minutes": 15,
                                "file_config": {
                                    "headerRow": 1,
                                    "dataStartRow": 2,
                                    "delimiter": ",",
                                    "identifierType": "name",
                                    "timestamp": {
                                        "key": "timestamp",
                                        "format": "ISO8601",
                                        "customFormat": None,
                                        "timezoneMode": "embeddedOffset",
                                        "timezone": None,
                                    },
                                },
                                "column_mappings": [],
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            state_path.write_text(
                AppStateFile(
                    cursors={
                        "job-1": JobCursor(
                            last_pushed_timestamp=datetime(
                                2026, 4, 13, tzinfo=timezone.utc
                            ),
                            last_pushed_row_index=5,
                            last_run_at=datetime(2026, 4, 13, 1, tzinfo=timezone.utc),
                            last_error=None,
                        )
                    },
                    logs={
                        "job-1": [
                            JobLogEntry(
                                timestamp=datetime(2026, 4, 13, 2, tzinfo=timezone.utc),
                                level="info",
                                message="Migrated log entry",
                            )
                        ]
                    },
                ).model_dump_json(indent=2),
                encoding="utf-8",
            )

            store, _ = self.create_store(tmpdir)
            config = store.load()

            self.assertEqual(len(config.jobs), 1)
            payload = json.loads(
                Path(tmpdir, "workspaces", "workspace-123.json").read_text(
                    encoding="utf-8"
                )
            )
            datasource = payload["datasources"][0]
            self.assertEqual(datasource["name"], "Migrated Job")
            self.assertEqual(datasource["last_pushed_row_index"], 5)
            self.assertEqual(datasource["recent_logs"][0]["message"], "Migrated log entry")

            rewritten_config = json.loads(config_path.read_text(encoding="utf-8"))
            self.assertNotIn("jobs", rewritten_config)


if __name__ == "__main__":
    unittest.main()
