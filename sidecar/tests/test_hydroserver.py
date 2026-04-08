from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest.mock import patch

import requests

from sidecar.api.models import ServerConfig
from sidecar.core.hydroserver import HydroServerService


class HydroServerServiceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.service = HydroServerService()

    def test_api_key_without_associated_workspaces_is_rejected(self) -> None:
        client = SimpleNamespace(
            workspaces=SimpleNamespace(
                list=lambda **_: SimpleNamespace(total_count=0, items=[])
            ),
        )

        with patch.object(self.service, "_build_client", return_value=client):
            result = self.service.test_connection(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="bad-key",
                    username="",
                    password="",
                )
            )

        self.assertFalse(result.ok)
        self.assertEqual(result.state, "error")
        self.assertIn("invalid or is not attached", result.message)

    def test_api_key_with_associated_workspace_connects_without_checking_datastreams(self) -> None:
        workspace = SimpleNamespace(uid="workspace-123")
        client = SimpleNamespace(
            workspaces=SimpleNamespace(
                list=lambda **kwargs: SimpleNamespace(
                    total_count=1 if kwargs.get("is_associated") else 99,
                    items=[workspace],
                )
            ),
        )

        with patch.object(self.service, "_build_client", return_value=client):
            result = self.service.test_connection(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="good-key",
                    username="",
                    password="",
                )
            )

        self.assertTrue(result.ok)
        self.assertEqual(result.state, "connected")
        self.assertEqual(result.workspace_id, "workspace-123")
        self.assertEqual(result.workspace_count, 1)
        self.assertEqual(result.datastream_count, 0)

    def test_list_datastreams_uses_stored_workspace_scope(self) -> None:
        datastream_calls: list[dict[str, object]] = []

        def list_datastreams(**kwargs):
            datastream_calls.append(kwargs)
            return SimpleNamespace(
                total_count=1,
                items=[SimpleNamespace(uid="stream-1", name="Datastream 1")],
            )

        client = SimpleNamespace(
            datastreams=SimpleNamespace(
                list=list_datastreams
            )
        )

        with patch.object(self.service, "_build_client", return_value=client):
            result = self.service.list_datastreams(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="good-key",
                    username="",
                    password="",
                    workspace_id="workspace-123",
                )
            )

        self.assertEqual(
            datastream_calls, [{"workspace": "workspace-123", "fetch_all": True}]
        )
        self.assertEqual(result[0].id, "stream-1")

    def test_list_datastreams_returns_related_summary_fields(self) -> None:
        client = SimpleNamespace(
            datastreams=SimpleNamespace(
                list=lambda **kwargs: SimpleNamespace(
                    total_count=1,
                    items=[
                        SimpleNamespace(
                            uid="stream-1",
                            name="Water level",
                            thing_id="thing-1",
                            sampled_medium="Water",
                            result_type="Measure",
                            thing=SimpleNamespace(uid="thing-1", name="River Site"),
                            observed_property=SimpleNamespace(name="Stage"),
                            processing_level=SimpleNamespace(definition="Raw"),
                            unit=SimpleNamespace(name="meter", symbol="m"),
                            sensor=SimpleNamespace(name="Pressure transducer"),
                        )
                    ],
                )
            )
        )

        with patch.object(self.service, "_build_client", return_value=client):
            result = self.service.list_datastreams(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="good-key",
                    username="",
                    password="",
                    workspace_id="workspace-123",
                )
            )

        self.assertEqual(len(result), 1)
        self.assertEqual(result[0].thing_id, "thing-1")
        self.assertEqual(result[0].thing_name, "River Site")
        self.assertEqual(result[0].observed_property_name, "Stage")
        self.assertEqual(result[0].processing_level_definition, "Raw")
        self.assertEqual(result[0].unit_symbol, "m")
        self.assertEqual(result[0].sensor_name, "Pressure transducer")
        self.assertEqual(result[0].sampled_medium, "Water")
        self.assertEqual(result[0].result_type, "Measure")

    def test_connection_error_returns_url_message(self) -> None:
        with patch.object(
            self.service,
            "_build_client",
            side_effect=requests.ConnectionError("boom"),
        ):
            result = self.service.test_connection(
                ServerConfig(
                    auth_type="apikey",
                    url="https://example.com",
                    api_key="any-key",
                    username="",
                    password="",
                )
            )

        self.assertFalse(result.ok)
        self.assertEqual(result.message, "Couldn't reach HydroServer. Check the server URL and try again.")


if __name__ == "__main__":
    unittest.main()
