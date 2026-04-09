from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Mapping
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
    workspace_name: str | None = None
    workspace_count: int = 0
    datastream_count: int = 0
    permissions_ok: bool = False


@dataclass
class HydroServerUrlCheck:
    ok: bool
    message: str
    instance_name: str | None = None


class HydroServerService:
    _DATASTREAM_PAGE_SIZE = 1000
    _DATASTREAM_CACHE_TTL = timedelta(minutes=5)

    def __init__(self) -> None:
        self._datastream_cache: dict[str, tuple[datetime, list[DatastreamSummary]]] = {}

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
            workspace_id, workspace_name, workspace_count = self._get_associated_workspace(
                client
            )

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
                    workspace_name=None,
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
                workspace_name=workspace_name,
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
            workspace_id, _, _ = self._get_associated_workspace(client)

        if not workspace_id:
            return []

        cache_key = self._datastream_cache_key(server, workspace_id)
        cached = self._get_cached_datastreams(cache_key)
        if cached is not None:
            return cached

        bootstrap_datastreams = self._list_datastreams_from_bootstrap(client, workspace_id)
        if bootstrap_datastreams is not None:
            self._set_cached_datastreams(cache_key, bootstrap_datastreams)
            return bootstrap_datastreams

        expanded_datastreams = self._list_datastreams_expanded(client, workspace_id)
        if expanded_datastreams is not None:
            self._set_cached_datastreams(cache_key, expanded_datastreams)
            return expanded_datastreams

        datastreams = self._collection_items(
            client.datastreams.list(workspace=workspace_id, fetch_all=True)
        )
        if not datastreams:
            return []

        thing_service = getattr(client, "things", None)
        observed_property_service = getattr(client, "observedproperties", None)
        processing_level_service = getattr(client, "processinglevels", None)
        unit_service = getattr(client, "units", None)
        sensor_service = getattr(client, "sensors", None)

        thing_by_id = self._build_resource_lookup(thing_service, workspace_id)
        observed_property_by_id = self._build_resource_lookup(
            observed_property_service, workspace_id
        )
        processing_level_by_id = self._build_resource_lookup(
            processing_level_service, workspace_id
        )
        unit_by_id = self._build_resource_lookup(unit_service, workspace_id)
        sensor_by_id = self._build_resource_lookup(sensor_service, workspace_id)

        summaries = [
            self._to_summary(
                item,
                thing_service=thing_service,
                thing_by_id=thing_by_id,
                observed_property_service=observed_property_service,
                observed_property_by_id=observed_property_by_id,
                processing_level_service=processing_level_service,
                processing_level_by_id=processing_level_by_id,
                unit_service=unit_service,
                unit_by_id=unit_by_id,
                sensor_service=sensor_service,
                sensor_by_id=sensor_by_id,
            )
            for item in datastreams
        ]
        self._set_cached_datastreams(cache_key, summaries)
        return summaries

    def _list_datastreams_from_bootstrap(
        self, client: object, workspace_id: str
    ) -> list[DatastreamSummary] | None:
        if not hasattr(client, "request"):
            return None

        try:
            response = client.request(
                "get",
                f"{getattr(client, 'base_route', '/api/data')}/datastreams/visualization-bootstrap",
                params={"workspace_id": workspace_id},
            )
        except Exception:
            return None

        payload = self._response_json(response)
        if not isinstance(payload, dict):
            return None

        datastreams_payload = self._value(payload, "datastreams")
        things_payload = self._value(payload, "things")
        observed_properties_payload = self._value(
            payload, "observed_properties", "observedProperties"
        )
        processing_levels_payload = self._value(
            payload, "processing_levels", "processingLevels"
        )

        if not isinstance(datastreams_payload, list) or not isinstance(things_payload, list):
            return None
        if not isinstance(observed_properties_payload, list):
            return None
        if not isinstance(processing_levels_payload, list):
            return None

        units_by_id = self._list_units_by_id(client, workspace_id)
        if units_by_id is None:
            return None

        things_by_id = {
            self._string_value(thing, "id", "uid"): thing for thing in things_payload
        }
        observed_properties_by_id = {
            self._string_value(observed_property, "id", "uid"): observed_property
            for observed_property in observed_properties_payload
        }
        processing_levels_by_id = {
            self._string_value(processing_level, "id", "uid"): processing_level
            for processing_level in processing_levels_payload
        }

        return [
            DatastreamSummary(
                id=self._string_value(datastream, "id", "uid"),
                name=self._string_value(datastream, "name") or "Unnamed datastream",
                thing_id=thing_id,
                thing_name=self._string_value(things_by_id.get(thing_id), "name"),
                observed_property_name=self._string_value(
                    observed_properties_by_id.get(observed_property_id), "name"
                ),
                processing_level_definition=self._string_value(
                    processing_levels_by_id.get(processing_level_id), "definition"
                ),
                unit_name=self._string_value(units_by_id.get(unit_id), "name"),
                unit_symbol=self._string_value(units_by_id.get(unit_id), "symbol"),
                sampled_medium="",
                sensor_name="",
                result_type="",
            )
            for datastream in datastreams_payload
            for thing_id in [self._string_value(datastream, "thing_id", "thingId")]
            for observed_property_id in [
                self._string_value(
                    datastream, "observed_property_id", "observedPropertyId"
                )
            ]
            for processing_level_id in [
                self._string_value(
                    datastream, "processing_level_id", "processingLevelId"
                )
            ]
            for unit_id in [self._string_value(datastream, "unit_id", "unitId")]
        ]

    def _list_units_by_id(
        self, client: object, workspace_id: str
    ) -> dict[str, object] | None:
        units_service = getattr(client, "units", None)
        if units_service is None or not hasattr(units_service, "list"):
            return None

        try:
            units = units_service.list(workspace=workspace_id, fetch_all=True)
        except Exception:
            return None

        return {
            unit_id: unit
            for unit in self._collection_items(units)
            if (unit_id := self._resource_id(unit))
        }

    def _list_datastreams_expanded(
        self, client: object, workspace_id: str
    ) -> list[DatastreamSummary] | None:
        if not hasattr(client, "request"):
            return None

        page = 1
        datastreams: list[DatastreamSummary] = []

        while True:
            try:
                response = client.request(
                    "get",
                    f"{getattr(client, 'base_route', '/api/data')}/datastreams",
                    params={
                        "workspace_id": workspace_id,
                        "expand_related": "true",
                        "page": page,
                        "page_size": self._DATASTREAM_PAGE_SIZE,
                    },
                )
            except Exception:
                return None

            payload = self._response_json(response)
            if not isinstance(payload, list):
                return None

            if not payload:
                break

            datastreams.extend(
                self._expanded_datastream_to_summary(item) for item in payload
            )

            total_pages = self._header_int(response, "X-Total-Pages")
            if total_pages is not None:
                if page >= total_pages:
                    break
            elif len(payload) < self._DATASTREAM_PAGE_SIZE:
                break

            page += 1

        return datastreams

    def _expanded_datastream_to_summary(self, item: object) -> DatastreamSummary:
        thing = self._value(item, "thing")
        observed_property = self._value(
            item, "observed_property", "observedProperty"
        )
        processing_level = self._value(
            item, "processing_level", "processingLevel"
        )
        unit = self._value(item, "unit")
        sensor = self._value(item, "sensor")

        thing_id = self._string_value(item, "thing_id", "thingId") or self._string_value(
            thing, "id", "uid"
        )
        observed_property_id = self._string_value(
            item, "observed_property_id", "observedPropertyId"
        ) or self._string_value(observed_property, "id", "uid")
        processing_level_id = self._string_value(
            item, "processing_level_id", "processingLevelId"
        ) or self._string_value(processing_level, "id", "uid")
        unit_id = self._string_value(item, "unit_id", "unitId") or self._string_value(
            unit, "id", "uid"
        )

        return DatastreamSummary(
            id=self._string_value(item, "id", "uid"),
            name=self._string_value(item, "name") or "Unnamed datastream",
            thing_id=thing_id,
            thing_name=self._string_value(thing, "name"),
            observed_property_name=self._string_value(observed_property, "name"),
            processing_level_definition=self._string_value(
                processing_level, "definition"
            ),
            unit_name=self._string_value(unit, "name"),
            unit_symbol=self._string_value(unit, "symbol"),
            sampled_medium=self._string_value(item, "sampled_medium", "sampledMedium"),
            sensor_name=self._string_value(sensor, "name"),
            result_type=self._string_value(item, "result_type", "resultType"),
        )

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

    def _get_associated_workspace(
        self, client
    ) -> tuple[str | None, str | None, int]:
        workspaces = self._list_associated_workspaces(client)
        workspace_count = self._collection_count(workspaces)
        first_workspace = next(iter(self._collection_items(workspaces)), None)
        if first_workspace is None:
            return None, None, workspace_count
        return (
            self._resource_id(first_workspace),
            self._string_attribute(first_workspace, "name"),
            workspace_count,
        )

    def _is_configured(self, server: ServerConfig) -> bool:
        if not server.url.strip():
            return False
        if server.auth_type == "userpass":
            return bool(server.username.strip() and server.password.strip())
        return bool(server.api_key.strip())

    def _to_summary(
        self,
        item: object,
        *,
        thing_service: object | None = None,
        thing_by_id: Mapping[str, object] | None = None,
        observed_property_service: object | None = None,
        observed_property_by_id: Mapping[str, object] | None = None,
        processing_level_service: object | None = None,
        processing_level_by_id: Mapping[str, object] | None = None,
        unit_service: object | None = None,
        unit_by_id: Mapping[str, object] | None = None,
        sensor_service: object | None = None,
        sensor_by_id: Mapping[str, object] | None = None,
    ) -> DatastreamSummary:
        datastream_id = getattr(item, "uid", None) or getattr(item, "id", "")
        name = getattr(item, "name", "Unnamed datastream")
        thing_id = self._string_attribute(item, "thing_id", "thingId")
        observed_property_id = self._string_attribute(
            item, "observed_property_id", "observedPropertyId"
        )
        processing_level_id = self._string_attribute(
            item, "processing_level_id", "processingLevelId"
        )
        unit_id = self._string_attribute(item, "unit_id", "unitId")
        sensor_id = self._string_attribute(item, "sensor_id", "sensorId")

        thing = self._related_resource(
            item, "thing", thing_id, thing_by_id, service=thing_service
        )
        observed_property = self._related_resource(
            item,
            "observed_property",
            observed_property_id,
            observed_property_by_id,
            service=observed_property_service,
        )
        processing_level = self._related_resource(
            item,
            "processing_level",
            processing_level_id,
            processing_level_by_id,
            service=processing_level_service,
        )
        unit = self._related_resource(
            item, "unit", unit_id, unit_by_id, service=unit_service
        )
        sensor = self._related_resource(
            item, "sensor", sensor_id, sensor_by_id, service=sensor_service
        )

        if not thing_id:
            thing_id = self._resource_id(thing)

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

    def _build_resource_lookup(
        self,
        service: object | None,
        workspace_id: str,
    ) -> dict[str, object]:
        if service is None or not hasattr(service, "list"):
            return {}

        try:
            collection = service.list(workspace=workspace_id, fetch_all=True)
        except Exception:
            return {}

        return {
            resource_id: resource
            for resource in self._collection_items(collection)
            if (resource_id := self._resource_id(resource))
        }

    def _related_resource(
        self,
        item: object,
        attribute: str,
        resource_id: str,
        lookup: Mapping[str, object] | None,
        *,
        service: object | None = None,
    ) -> object | None:
        cached = self._first_attribute(
            item, f"_{attribute}", f"_{self._to_camel_case(attribute)}"
        )
        if cached is not None:
            return cached

        direct = self._first_attribute(item, attribute, self._to_camel_case(attribute))
        if direct is not None:
            return direct

        if resource_id and lookup is not None:
            resource = lookup.get(resource_id)
            if resource is not None:
                return resource

        if resource_id and service is not None and hasattr(service, "get"):
            try:
                resource = service.get(resource_id)
            except Exception:
                return None

            if resource is not None and isinstance(lookup, dict):
                lookup[resource_id] = resource
            return resource

        return None

    def _safe_attribute(self, item: object, attribute: str) -> object | None:
        descriptor = getattr(type(item), "__dict__", {}).get(attribute)
        if isinstance(descriptor, property):
            return None

        try:
            return getattr(item, attribute, None)
        except Exception:
            return None

    def _string_attribute(self, item: object, *attributes: str) -> str:
        value = self._first_attribute(item, *attributes)
        if value is None:
            return ""
        return str(value)

    def _value(self, item: object, *attributes: str) -> object | None:
        if isinstance(item, dict):
            for attribute in attributes:
                if attribute in item and item[attribute] is not None:
                    return item[attribute]
            return None

        return self._first_attribute(item, *attributes)

    def _string_value(self, item: object, *attributes: str) -> str:
        value = self._value(item, *attributes)
        if value is None:
            return ""
        return str(value)

    def _first_attribute(self, item: object, *attributes: str) -> object | None:
        for attribute in attributes:
            value = self._safe_attribute(item, attribute)
            if value is not None:
                return value
        return None

    def _to_camel_case(self, value: str) -> str:
        head, *tail = value.split("_")
        return "".join([head, *[part.capitalize() for part in tail]])

    def _resource_id(self, item: object) -> str:
        if item is None:
            return ""

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

    def _header_int(self, response: requests.Response, header: str) -> int | None:
        value = response.headers.get(header)
        if value is None:
            return None

        try:
            return int(value)
        except (TypeError, ValueError):
            return None

    def _datastream_cache_key(self, server: ServerConfig, workspace_id: str) -> str:
        credential = server.api_key if server.auth_type == "apikey" else server.username
        return "|".join(
            [
                server.auth_type,
                server.url.strip().rstrip("/"),
                workspace_id,
                credential.strip(),
            ]
        )

    def _get_cached_datastreams(
        self, cache_key: str
    ) -> list[DatastreamSummary] | None:
        cached = self._datastream_cache.get(cache_key)
        if cached is None:
            return None

        cached_at, datastreams = cached
        if datetime.now(timezone.utc) - cached_at > self._DATASTREAM_CACHE_TTL:
            self._datastream_cache.pop(cache_key, None)
            return None

        return datastreams

    def _set_cached_datastreams(
        self, cache_key: str, datastreams: list[DatastreamSummary]
    ) -> None:
        self._datastream_cache[cache_key] = (
            datetime.now(timezone.utc),
            datastreams,
        )
