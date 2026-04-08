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
    workspace_id: str | None = None
    workspace_count: int = 0
    datastream_count: int = 0
    permissions_ok: bool = False


@dataclass
class HydroServerUrlCheck:
    ok: bool
    message: str
    instance_name: str | None = None


class HydroServerService:
    def validate_url(self, url: str) -> HydroServerUrlCheck:
        normalized_url = url.strip().rstrip("/")
        if not normalized_url:
            return HydroServerUrlCheck(
                ok=False,
                message="Enter the HydroServer URL.",
            )

        auth_probe_url = f"{normalized_url}/api/auth/app/session"
        data_probe_url = f"{normalized_url}/api/data/workspaces"

        try:
            auth_response = requests.get(
                auth_probe_url,
                headers={"Accept": "application/json"},
                timeout=15,
                allow_redirects=True,
            )
            if self._looks_like_hydroserver_auth_response(auth_response):
                instance_name = self._instance_name(normalized_url)
                return HydroServerUrlCheck(
                    ok=True,
                    message=f"HydroServer API detected at {instance_name}.",
                    instance_name=instance_name,
                )

            data_response = requests.get(
                data_probe_url,
                headers={"Accept": "application/json"},
                timeout=15,
                allow_redirects=True,
            )
            if self._looks_like_hydroserver_data_response(data_response):
                instance_name = self._instance_name(normalized_url)
                return HydroServerUrlCheck(
                    ok=True,
                    message=f"HydroServer API detected at {instance_name}.",
                    instance_name=instance_name,
                )

            return HydroServerUrlCheck(
                ok=False,
                message="That URL responded, but it doesn't look like a HydroServer instance exposing the expected API.",
            )
        except (requests.ConnectionError, requests.Timeout):
            return HydroServerUrlCheck(
                ok=False,
                message="Couldn't reach that URL. Check the server URL and try again.",
            )
        except requests.RequestException:
            return HydroServerUrlCheck(
                ok=False,
                message="Couldn't validate that HydroServer URL right now.",
            )

    def test_connection(self, server: ServerConfig) -> HydroServerCheck:
        if not self._is_configured(server):
            return HydroServerCheck(
                ok=False,
                state="not_configured",
                message="Enter the HydroServer URL and a valid set of credentials.",
            )

        try:
            client = self._build_client(server)
            workspace_id, workspace_count = self._get_associated_workspace_id(client)

            if not workspace_id:
                return HydroServerCheck(
                    ok=False,
                    state="error",
                    message=(
                        "That API key is invalid or is not attached to any accessible workspace. "
                        "Check the API key permissions and try again."
                    ),
                    instance_name=self._instance_name(server.url),
                    workspace_id=None,
                    workspace_count=workspace_count,
                    datastream_count=0,
                    permissions_ok=False,
                )

            instance_name = self._instance_name(server.url)
            return HydroServerCheck(
                ok=True,
                state="connected",
                message=f"Connected to {instance_name}.",
                instance_name=instance_name,
                workspace_id=workspace_id,
                workspace_count=workspace_count,
                datastream_count=0,
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
        workspace_id = server.workspace_id.strip()
        if not workspace_id:
            workspace_id, _ = self._get_associated_workspace_id(client)

        if not workspace_id:
            return []

        datastreams = client.datastreams.list(workspace=workspace_id, fetch_all=True)
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

    def _list_associated_workspaces(self, client):
        return client.workspaces.list(page_size=25, is_associated=True)

    def _get_associated_workspace_id(self, client) -> tuple[str | None, int]:
        workspaces = self._list_associated_workspaces(client)
        workspace_count = self._collection_count(workspaces)
        first_workspace = next(iter(self._collection_items(workspaces)), None)
        if first_workspace is None:
            return None, workspace_count
        return self._resource_id(first_workspace), workspace_count

    def _is_configured(self, server: ServerConfig) -> bool:
        if not server.url.strip():
            return False
        if server.auth_type == "userpass":
            return bool(server.username.strip() and server.password.strip())
        return bool(server.api_key.strip())

    def _to_summary(self, item: object) -> DatastreamSummary:
        datastream_id = getattr(item, "uid", None) or getattr(item, "id", "")
        name = getattr(item, "name", "Unnamed datastream")
        thing = self._related_resource(item, "thing")
        observed_property = self._related_resource(item, "observed_property")
        processing_level = self._related_resource(item, "processing_level")
        unit = self._related_resource(item, "unit")
        sensor = self._related_resource(item, "sensor")

        thing_id = (
            getattr(item, "thing_id", None)
            or getattr(thing, "uid", None)
            or getattr(thing, "id", "")
        )

        return DatastreamSummary(
            id=str(datastream_id),
            name=str(name),
            thing_id=str(thing_id or ""),
            thing_name=str(getattr(thing, "name", "") or ""),
            observed_property_name=str(getattr(observed_property, "name", "") or ""),
            processing_level_definition=str(
                getattr(processing_level, "definition", "") or ""
            ),
            unit_name=str(getattr(unit, "name", "") or ""),
            unit_symbol=str(getattr(unit, "symbol", "") or ""),
            sampled_medium=str(
                getattr(item, "sampled_medium", None)
                or getattr(item, "sampledMedium", "")
                or ""
            ),
            sensor_name=str(getattr(sensor, "name", "") or ""),
            result_type=str(
                getattr(item, "result_type", None)
                or getattr(item, "resultType", "")
                or ""
            ),
        )

    def _related_resource(self, item: object, attribute: str) -> object | None:
        try:
            return getattr(item, attribute, None)
        except Exception:
            return None

    def _resource_id(self, item: object) -> str:
        resource_id = getattr(item, "uid", None) or getattr(item, "id", "")
        return str(resource_id)

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

    def _looks_like_hydroserver_auth_response(self, response: requests.Response) -> bool:
        if response.status_code not in {200, 401, 403, 405, 422}:
            return False

        payload = self._response_json(response)
        if not isinstance(payload, dict):
            return False

        meta = payload.get("meta")
        data = payload.get("data")
        detail = payload.get("detail")

        return (
            isinstance(meta, dict)
            and "is_authenticated" in meta
            or isinstance(data, dict)
            and "flows" in data
            or isinstance(detail, list)
        )

    def _looks_like_hydroserver_data_response(self, response: requests.Response) -> bool:
        if response.status_code not in {200, 401, 403}:
            return False

        payload = self._response_json(response)
        return isinstance(payload, list) or (
            isinstance(payload, dict)
            and ("detail" in payload or "status" in payload)
        )

    def _response_json(self, response: requests.Response) -> object | None:
        content_type = response.headers.get("content-type", "").lower()
        if "json" not in content_type:
            return None

        try:
            return response.json()
        except ValueError:
            return None
