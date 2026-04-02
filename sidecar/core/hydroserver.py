from __future__ import annotations

from dataclasses import dataclass
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
    workspace_count: int = 0
    datastream_count: int = 0
    permissions_ok: bool = False


class HydroServerService:
    def test_connection(self, server: ServerConfig) -> HydroServerCheck:
        if not self._is_configured(server):
            return HydroServerCheck(
                ok=False,
                state="not_configured",
                message="Enter the HydroServer URL and a valid set of credentials.",
            )

        try:
            client = self._build_client(server)
            workspaces = client.workspaces.list(page_size=25)
            datastreams = client.datastreams.list(page_size=100)
            client.orchestrationsystems.list(page_size=25)

            workspace_count = self._collection_count(workspaces)
            datastream_count = self._collection_count(datastreams)

            if workspace_count == 0:
                return HydroServerCheck(
                    ok=False,
                    state="error",
                    message="This API key is not attached to any accessible workspace. Check the key permissions and try again.",
                    instance_name=self._instance_name(server.url),
                    workspace_count=workspace_count,
                    datastream_count=datastream_count,
                    permissions_ok=False,
                )

            if datastream_count == 0:
                return HydroServerCheck(
                    ok=False,
                    state="error",
                    message="No datastreams are available to this API key. Create a datastream in HydroServer or update the key permissions, then try again.",
                    instance_name=self._instance_name(server.url),
                    workspace_count=workspace_count,
                    datastream_count=datastream_count,
                    permissions_ok=False,
                )

            instance_name = self._instance_name(server.url)
            return HydroServerCheck(
                ok=True,
                state="connected",
                message=f"Connected to {instance_name}. {datastream_count} datastreams are available for mapping.",
                instance_name=instance_name,
                workspace_count=workspace_count,
                datastream_count=datastream_count,
                permissions_ok=True,
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
                    message="These credentials are invalid or do not have the permissions the loader needs. Make sure they can access workspaces, datastreams, and orchestration systems.",
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
        if not self._is_configured(server):
            return []

        client = self._build_client(server)
        datastreams = client.datastreams.list(page_size=100)
        return [self._to_summary(item) for item in self._collection_items(datastreams)]

    def _build_client(self, server: ServerConfig):
        if HydroServer is None:
            raise RuntimeError("hydroserverpy is not installed.")
        if server.auth_type == "userpass":
            return HydroServer(
                host=server.url,
                email=server.username,
                password=server.password,
            )
        return HydroServer(host=server.url, apikey=server.api_key)

    def _is_configured(self, server: ServerConfig) -> bool:
        if not server.url.strip():
            return False
        if server.auth_type == "userpass":
            return bool(server.username.strip() and server.password.strip())
        return bool(server.api_key.strip())

    def _to_summary(self, item: object) -> DatastreamSummary:
        datastream_id = getattr(item, "uid", None) or getattr(item, "id", "")
        name = getattr(item, "name", "Unnamed datastream")
        return DatastreamSummary(id=str(datastream_id), name=str(name))

    def _instance_name(self, url: str) -> str:
        parsed = urlparse(url)
        return parsed.netloc or url

    def _collection_count(self, collection: object) -> int:
        total_count = getattr(collection, "total_count", None)
        if isinstance(total_count, int):
            return total_count
        return len(self._collection_items(collection))

    def _collection_items(self, collection: object) -> list[object]:
        items = getattr(collection, "items", None)
        if isinstance(items, list):
            return items
        return []
