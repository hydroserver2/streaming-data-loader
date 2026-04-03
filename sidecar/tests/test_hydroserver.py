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
            datastreams=SimpleNamespace(
                list=lambda **_: SimpleNamespace(total_count=3, items=[object()] * 3)
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

    def test_api_key_with_associated_workspace_and_datastreams_connects(self) -> None:
        client = SimpleNamespace(
            workspaces=SimpleNamespace(
                list=lambda **kwargs: SimpleNamespace(
                    total_count=1 if kwargs.get("is_associated") else 99,
                    items=[object()],
                )
            ),
            datastreams=SimpleNamespace(
                list=lambda **_: SimpleNamespace(total_count=2, items=[object(), object()])
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
        self.assertEqual(result.workspace_count, 1)
        self.assertEqual(result.datastream_count, 2)

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
