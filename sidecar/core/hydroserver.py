from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable
from urllib.parse import urlparse

import requests

from sidecar.api.models import DatastreamSummary, ServerConfig

try:
    from hydroserverpy import HydroServer
except ImportError:  # pragma: no cover - bootstrap handles installation
    HydroServer = None  # type: ignore[assignment]


@dataclass
class HydroServerCheck:
    ok: bool
    message: str
    state: str
    instance_name: str | None = None


class HydroServerService:
    def test_connection(self, server: ServerConfig) -> HydroServerCheck:
        if not server.url.strip() or not server.api_key.strip():
            return HydroServerCheck(
                ok=False,
                state="not_configured",
                message="Enter both the HydroServer URL and API key.",
            )

        try:
            client = self._build_client(server)
            client.datastreams.list(page_size=1)
            instance_name = self._instance_name(server.url)
            return HydroServerCheck(
                ok=True,
                state="connected",
                message=f"Connected to {instance_name}.",
                instance_name=instance_name,
            )
        except requests.ConnectionError:
            return HydroServerCheck(
                ok=False,
                state="error",
                message="Couldn't reach HydroServer. Check the server URL and try again.",
            )
        except requests.HTTPError as exc:
            status_code = getattr(getattr(exc, "response", None), "status_code", None)
            if status_code in {401, 403}:
                return HydroServerCheck(
                    ok=False,
                    state="error",
                    message="Invalid API key. Double-check the key in your HydroServer account settings.",
                )
            return HydroServerCheck(
                ok=False,
                state="error",
                message="HydroServer returned an error while testing the connection. Try again in a moment.",
            )
        except Exception:
            return HydroServerCheck(
                ok=False,
                state="error",
                message="Couldn't complete the HydroServer connection test.",
            )

    def list_datastreams(self, server: ServerConfig) -> list[DatastreamSummary]:
        if not server.url.strip() or not server.api_key.strip():
            return []

        client = self._build_client(server)
        datastreams = client.datastreams.list(page_size=100)
        return [self._to_summary(item) for item in datastreams]

    def _build_client(self, server: ServerConfig):
        if HydroServer is None:
            raise RuntimeError("hydroserverpy is not installed.")
        return HydroServer(host=server.url, apikey=server.api_key)

    def _to_summary(self, item: object) -> DatastreamSummary:
        datastream_id = getattr(item, "uid", None) or getattr(item, "id", "")
        name = getattr(item, "name", "Unnamed datastream")
        return DatastreamSummary(id=str(datastream_id), name=str(name))

    def _instance_name(self, url: str) -> str:
        parsed = urlparse(url)
        return parsed.netloc or url
