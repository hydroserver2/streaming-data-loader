use std::{collections::HashMap, sync::Mutex, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    Client, Method, Response, StatusCode,
};
use serde_json::{json, Value};

use crate::models::{
    normalize_url, AuthType, ConnectionState, ConnectionTestResponse, DatastreamDetail,
    DatastreamObservedPropertyDetail, DatastreamProcessingLevelDetail, DatastreamSensorDetail,
    DatastreamSummary, DatastreamThingDetail, DatastreamThingLocationDetail, DatastreamUnitDetail,
    ServerConfig, ServerUrlValidationResponse,
};

const BASE_ROUTE: &str = "/api/data";
const DATASTREAM_PAGE_SIZE: usize = 1000;
const DATASTREAM_INCLUDE: &str = "monitoringSite,observedProperty,processingLevel,unit,method";
const DATASTREAM_CACHE_TTL_SECONDS: i64 = 300;

type DatastreamCacheEntry = (DateTime<Utc>, Vec<DatastreamSummary>);
type DatastreamCache = HashMap<String, DatastreamCacheEntry>;

#[derive(Debug, Clone)]
pub struct ObservationPayloadRow {
    pub phenomenon_time: DateTime<Utc>,
    pub result: Value,
}

pub struct HydroServerService {
    http: Client,
    datastream_cache: Mutex<DatastreamCache>,
}

impl HydroServerService {
    pub fn new() -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|err| err.to_string())?;

        Ok(Self {
            http,
            datastream_cache: Mutex::new(HashMap::new()),
        })
    }

    fn session(&self, server: ServerConfig) -> HydroServerSession {
        HydroServerSession::new(self.http.clone(), server)
    }

    pub async fn validate_url(&self, url: &str) -> ServerUrlValidationResponse {
        let normalized_url = normalize_url(url);
        if normalized_url.is_empty() {
            return ServerUrlValidationResponse {
                ok: false,
                message: "Enter the HydroServer URL.".to_string(),
                instance_name: None,
            };
        }

        let data_probe_url = format!("{normalized_url}{BASE_ROUTE}/workspaces");

        match self
            .http
            .get(&data_probe_url)
            .query(&[("limit", "1")])
            .header(ACCEPT, "application/json")
            .send()
            .await
        {
            Ok(response) => {
                if looks_like_hydroserver_data_response(response).await {
                    let instance_name = instance_name(&normalized_url);
                    ServerUrlValidationResponse {
                        ok: true,
                        message: format!("HydroServer API detected at {instance_name}."),
                        instance_name: Some(instance_name),
                    }
                } else {
                    ServerUrlValidationResponse {
                        ok: false,
                        message: "That URL responded, but it doesn't look like a HydroServer instance exposing the expected API.".to_string(),
                        instance_name: None,
                    }
                }
            }
            Err(err) if err.is_connect() || err.is_timeout() => ServerUrlValidationResponse {
                ok: false,
                message: "Couldn't reach that URL. Check the server URL and try again.".to_string(),
                instance_name: None,
            },
            Err(_) => ServerUrlValidationResponse {
                ok: false,
                message: "Couldn't validate that HydroServer URL right now.".to_string(),
                instance_name: None,
            },
        }
    }

    pub async fn test_connection(&self, server: &ServerConfig) -> ConnectionTestResponse {
        if !server.is_configured() {
            return ConnectionTestResponse {
                ok: false,
                state: ConnectionState::NotConfigured,
                message: "Enter the HydroServer URL and a valid set of credentials.".to_string(),
                invalid_field: None,
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            };
        }

        let mut session = self.session(server.clone().normalized());
        match session.associated_workspaces().await {
            Ok(workspaces) => {
                let workspace_count = workspaces.len() as u32;
                let selected_workspace = match server.auth_type {
                    AuthType::Apikey => resolve_api_key_workspace(server, &workspaces),
                    AuthType::Userpass => resolve_userpass_workspace(server, &workspaces),
                };

                match selected_workspace {
                    Ok(Some((workspace_id, workspace_name))) => {
                        let instance_name = instance_name(&server.url);
                        ConnectionTestResponse {
                            ok: true,
                            state: ConnectionState::Connected,
                            message: format!("Connected to {instance_name}."),
                            invalid_field: None,
                            instance_name: Some(instance_name),
                            workspace_id: Some(workspace_id),
                            workspace_name: Some(workspace_name),
                            workspace_count,
                            datastream_count: 0,
                            permissions_ok: true,
                        }
                    }
                    Ok(None) => ConnectionTestResponse {
                        ok: false,
                        state: ConnectionState::Error,
                        message: "That API key is invalid or is not attached to any accessible workspace. Check the API key permissions and try again.".to_string(),
                        invalid_field: Some("api_key".to_string()),
                        instance_name: Some(instance_name(&server.url)),
                        workspace_id: None,
                        workspace_name: None,
                        workspace_count,
                        datastream_count: 0,
                        permissions_ok: false,
                    },
                    Err((invalid_field, message)) => ConnectionTestResponse {
                        ok: false,
                        state: ConnectionState::Error,
                        message,
                        invalid_field: Some(invalid_field.to_string()),
                        instance_name: Some(instance_name(&server.url)),
                        workspace_id: None,
                        workspace_name: None,
                        workspace_count,
                        datastream_count: 0,
                        permissions_ok: false,
                    },
                }
            }
            Err(RequestError::Connection) | Err(RequestError::Timeout) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "Couldn't reach HydroServer. Check the server URL and try again.".to_string(),
                invalid_field: Some("url".to_string()),
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            },
            Err(RequestError::Http {
                status: Some(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
                ..
            }) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "These credentials are invalid or do not have the permissions the loader needs. Make sure they can access workspaces, datastreams, and observations.".to_string(),
                invalid_field: Some(match server.auth_type {
                    AuthType::Apikey => "api_key".to_string(),
                    AuthType::Userpass => "username".to_string(),
                }),
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            },
            Err(RequestError::Http { .. }) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "HydroServer returned an error while testing the connection. Try again in a moment.".to_string(),
                invalid_field: None,
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            },
            Err(RequestError::Other(_)) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "Couldn't complete the HydroServer connection test.".to_string(),
                invalid_field: None,
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            },
        }
    }

    pub async fn list_datastreams(
        &self,
        server: &ServerConfig,
        force: bool,
    ) -> Result<Vec<DatastreamSummary>, String> {
        if !server.is_configured() {
            return Ok(Vec::new());
        }

        let normalized = server.clone().normalized();
        let mut session = self.session(normalized.clone());
        let workspace_id = if normalized.workspace_id.is_empty() {
            session
                .associated_workspace()
                .await
                .map_err(|err| err.to_string())?
                .0
                .unwrap_or_default()
        } else {
            normalized.workspace_id.clone()
        };

        if workspace_id.is_empty() {
            return Ok(Vec::new());
        }

        let cache_key = datastream_cache_key(&normalized, &workspace_id);
        if !force {
            if let Some(cached) = self.cached_datastreams(&cache_key) {
                return Ok(cached);
            }
        }

        let datastreams = session
            .fetch_all_collection(
                &format!("{BASE_ROUTE}/datastreams"),
                &[
                    ("workspace_id", workspace_id.clone()),
                    ("include", DATASTREAM_INCLUDE.to_string()),
                ],
            )
            .await
            .map_err(|err| err.to_string())?;
        let summaries = datastreams
            .iter()
            .map(expanded_datastream_to_summary)
            .collect::<Vec<_>>();

        self.set_cached_datastreams(&cache_key, summaries.clone());
        Ok(summaries)
    }

    pub async fn get_datastream_detail(
        &self,
        server: &ServerConfig,
        datastream_id: &str,
    ) -> Result<DatastreamDetail, String> {
        if !server.is_configured() {
            return Err("Connect to HydroServer before loading datastream metadata.".to_string());
        }

        let normalized = server.clone().normalized();
        let mut session = self.session(normalized.clone());
        let workspace_id = if normalized.workspace_id.is_empty() {
            session
                .associated_workspace()
                .await
                .map_err(|err| err.to_string())?
                .0
                .unwrap_or_default()
        } else {
            normalized.workspace_id.clone()
        };

        let payload = session
            .request_json(
                Method::GET,
                &format!("{BASE_ROUTE}/datastreams/{datastream_id}"),
                &[
                    ("workspace_id", workspace_id),
                    ("include", DATASTREAM_INCLUDE.to_string()),
                ],
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

        let mut item = response_item(&payload)
            .map_err(|err| err.to_string())?
            .clone();
        attach_datastream_relations(std::slice::from_mut(&mut item), &payload);
        Ok(expanded_datastream_to_detail(&item))
    }

    /// Fetch the datastream's current `phenomenonEndTime` — the timestamp of
    /// the latest observation HydroServer has stored. Used to reconcile our
    /// cursor after a bulk-insert conflict: observations at or before this
    /// watermark are confirmed durable on the server. `Ok(None)` means the
    /// datastream has no observations yet.
    pub(crate) async fn fetch_phenomenon_end_time(
        &self,
        server: &ServerConfig,
        datastream_id: &str,
    ) -> Result<Option<DateTime<Utc>>, RequestError> {
        let normalized = server.clone().normalized();
        let mut session = self.session(normalized.clone());

        let mut params: Vec<(&str, String)> = Vec::new();
        if !normalized.workspace_id.is_empty() {
            params.push(("workspace_id", normalized.workspace_id.clone()));
        }

        let payload = session
            .request_json(
                Method::GET,
                &format!("{BASE_ROUTE}/datastreams/{datastream_id}"),
                &params,
                None,
            )
            .await?;

        Ok(parse_phenomenon_end_time(response_item(&payload)?))
    }

    pub(crate) async fn post_observations_batch(
        &self,
        server: &ServerConfig,
        datastream_id: &str,
        observations: &[ObservationPayloadRow],
    ) -> Result<(), RequestError> {
        if observations.is_empty() {
            return Ok(());
        }

        let mut session = self.session(server.clone().normalized());
        let body = json!({
            "datastreamId": datastream_id,
            "fields": ["phenomenonTime", "result"],
            "data": observations
                .iter()
                .map(|row| json!([row.phenomenon_time.to_rfc3339(), row.result]))
                .collect::<Vec<_>>(),
        });

        session
            .request_void(
                Method::POST,
                &format!("{BASE_ROUTE}/observations/bulk-create"),
                &[("mode", "insert".to_string())],
                Some(body),
            )
            .await
    }

    fn cached_datastreams(&self, cache_key: &str) -> Option<Vec<DatastreamSummary>> {
        let mut cache = self.datastream_cache.lock().ok()?;
        let (cached_at, datastreams) = cache.get(cache_key)?.clone();
        if Utc::now().signed_duration_since(cached_at).num_seconds() > DATASTREAM_CACHE_TTL_SECONDS
        {
            cache.remove(cache_key);
            return None;
        }
        Some(datastreams)
    }

    fn set_cached_datastreams(&self, cache_key: &str, datastreams: Vec<DatastreamSummary>) {
        if let Ok(mut cache) = self.datastream_cache.lock() {
            cache.insert(cache_key.to_string(), (Utc::now(), datastreams));
        }
    }
}

struct HydroServerSession {
    http: Client,
    server: ServerConfig,
}

impl HydroServerSession {
    fn new(http: Client, server: ServerConfig) -> Self {
        Self { http, server }
    }

    async fn associated_workspace(
        &mut self,
    ) -> Result<(Option<String>, Option<String>, u32), RequestError> {
        let workspaces = self.associated_workspaces().await?;
        let workspace_count = workspaces.len() as u32;
        let first_workspace = workspaces.first();
        Ok((
            first_workspace.map(|(id, _)| id.clone()),
            first_workspace.map(|(_, name)| name.clone()),
            workspace_count,
        ))
    }

    async fn associated_workspaces(&mut self) -> Result<Vec<(String, String)>, RequestError> {
        let items = self
            .fetch_all_collection(
                &format!("{BASE_ROUTE}/workspaces"),
                &[("is_associated", "true".to_string())],
            )
            .await?;

        Ok(items
            .into_iter()
            .filter_map(|item| {
                let id = string_value(&item, &["id", "uid"])?;
                let name = string_value(&item, &["name"])?;
                Some((id, name))
            })
            .collect())
    }

    async fn fetch_all_collection(
        &mut self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<Value>, RequestError> {
        let mut offset = 0_u64;
        let mut items = Vec::new();

        loop {
            let mut page_params = params.to_vec();
            page_params.push(("offset", offset.to_string()));
            page_params.push(("limit", DATASTREAM_PAGE_SIZE.to_string()));
            let payload = self
                .request_json(Method::GET, path, &page_params, None)
                .await?;
            let mut page_items = payload
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    RequestError::Other(
                        "HydroServer returned an invalid collection response.".to_string(),
                    )
                })?
                .clone();
            let total_count = payload
                .pointer("/meta/totalCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    RequestError::Other(
                        "HydroServer returned invalid pagination metadata.".to_string(),
                    )
                })?;

            if page_items.is_empty() && offset < total_count {
                return Err(RequestError::Other(
                    "HydroServer returned an empty page before the collection was complete."
                        .to_string(),
                ));
            }
            offset += page_items.len() as u64;
            attach_datastream_relations(&mut page_items, &payload);
            items.extend(page_items);
            if offset >= total_count {
                break;
            }
        }

        Ok(items)
    }

    async fn request_json(
        &mut self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, RequestError> {
        let response = self.request_response(method, path, params, body).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        response
            .json::<Value>()
            .await
            .map_err(|err| RequestError::Other(err.to_string()))
    }

    async fn request_void(
        &mut self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<(), RequestError> {
        self.request_response(method, path, params, body)
            .await
            .map(|_| ())
    }

    async fn request_response(
        &mut self,
        method: Method,
        path: &str,
        params: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Response, RequestError> {
        let url = build_url(&self.server.url, path);

        let mut request = self
            .http
            .request(method, &url)
            .header(ACCEPT, "application/json");
        if !params.is_empty() {
            request = request.query(params);
        }
        if let Some(payload) = body {
            request = request.json(&payload);
        }
        request = match self.server.auth_type {
            AuthType::Apikey => request.header("X-API-Key", &self.server.api_key),
            // HydroServer v2 removed the app/session endpoint. The data API
            // accepts email/password directly through HTTP Basic authentication.
            AuthType::Userpass => {
                request.basic_auth(&self.server.username, Some(&self.server.password))
            }
        };
        let response = request.send().await.map_err(|err| {
            if err.is_connect() {
                RequestError::Connection
            } else if err.is_timeout() {
                RequestError::Timeout
            } else {
                RequestError::Other(err.to_string())
            }
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        Err(RequestError::Http {
            status: Some(response.status()),
            message: response_error_message(response).await,
        })
    }
}

#[derive(Debug)]
pub(crate) enum RequestError {
    Connection,
    Timeout,
    Http {
        status: Option<StatusCode>,
        message: String,
    },
    Other(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Connection => write!(f, "Couldn't reach HydroServer."),
            RequestError::Timeout => write!(f, "HydroServer request timed out."),
            RequestError::Http { message, .. } | RequestError::Other(message) => {
                write!(f, "{message}")
            }
        }
    }
}

impl RequestError {
    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            Self::Connection | Self::Timeout => true,
            Self::Http {
                status: Some(status),
                ..
            } => status.is_server_error(),
            Self::Http { .. } | Self::Other(_) => false,
        }
    }

    pub(crate) fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::Http {
                status: Some(status),
                ..
            } if *status == StatusCode::CONFLICT
        )
    }
}

/// Read `phenomenonEndTime` (the API serializes camelCase; the snake_case
/// variant is accepted too) from a datastream payload as a UTC timestamp.
fn parse_phenomenon_end_time(payload: &Value) -> Option<DateTime<Utc>> {
    payload
        .get("phenomenonEndTime")
        .or_else(|| payload.get("phenomenon_end_time"))
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn resolve_api_key_workspace(
    server: &ServerConfig,
    workspaces: &[(String, String)],
) -> Result<Option<(String, String)>, (&'static str, String)> {
    if workspaces.is_empty() {
        return Ok(None);
    }

    // If the user previously saved a workspace, require that the current API
    // key still has access to it. Without this check, rotating the key to one
    // attached to a different workspace would silently redirect uploads to
    // that other workspace (bug_008).
    let requested_workspace_id = server.workspace_id.trim();
    if !requested_workspace_id.is_empty() {
        return workspaces
            .iter()
            .find(|(id, _)| id == requested_workspace_id)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                (
                    "api_key",
                    "This API key does not have access to the saved workspace. The key may have been rotated or its permissions changed. Re-select a workspace this key can access.".to_string(),
                )
            });
    }

    // No saved workspace — this is the initial connection. Pick the first
    // accessible one so the UI can offer it as the default.
    Ok(workspaces.first().cloned())
}

fn resolve_userpass_workspace(
    server: &ServerConfig,
    workspaces: &[(String, String)],
) -> Result<Option<(String, String)>, (&'static str, String)> {
    if workspaces.is_empty() {
        return Err((
            "workspace_name",
            "These credentials are valid, but this account does not have edit access to any related workspaces.".to_string(),
        ));
    }

    let requested_workspace_id = server.workspace_id.trim();
    if !requested_workspace_id.is_empty() {
        return workspaces
            .iter()
            .find(|(id, _)| id == requested_workspace_id)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                (
                    "workspace_name",
                    "The saved workspace is no longer available to this account. Enter one of your related workspace names and try again.".to_string(),
                )
            });
    }

    let requested_workspace_name = server.workspace_name.trim();
    if requested_workspace_name.is_empty() {
        return Err((
            "workspace_name",
            "Please enter a workspace name.".to_string(),
        ));
    }

    if let Some(workspace) = workspaces
        .iter()
        .find(|(_, name)| name.trim() == requested_workspace_name)
    {
        return Ok(Some(workspace.clone()));
    }

    if let Some(workspace) = workspaces
        .iter()
        .find(|(_, name)| name.trim().eq_ignore_ascii_case(requested_workspace_name))
    {
        return Ok(Some(workspace.clone()));
    }

    Err((
        "workspace_name",
        format!(
            "No related workspace named \"{requested_workspace_name}\" was found for this account. Check the workspace name and try again."
        ),
    ))
}

fn build_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        normalize_url(base_url),
        path.trim_start_matches('/')
    )
}

fn instance_name(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| url.to_string())
}

async fn looks_like_hydroserver_data_response(response: Response) -> bool {
    if response.status() != StatusCode::OK {
        return false;
    }
    let Some(payload) = response_json(response).await else {
        return false;
    };
    payload.get("data").and_then(Value::as_array).is_some()
        && ["limit", "offset", "totalCount"].iter().all(|key| {
            payload
                .get("meta")
                .and_then(|meta| meta.get(key))
                .and_then(Value::as_u64)
                .is_some()
        })
}

async fn response_json(response: Response) -> Option<Value> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.contains("json") {
        return None;
    }

    response.json::<Value>().await.ok()
}

async fn response_error_message(response: Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|payload| {
            payload
                .get("message")
                .or_else(|| payload.get("detail"))
                .cloned()
        })
        .map(format_error_detail);

    detail.unwrap_or_else(|| format!("HydroServer returned status {}.", status.as_u16()))
}

fn format_error_detail(detail: Value) -> String {
    match detail {
        Value::String(message) if !message.trim().is_empty() => message,
        Value::Array(items) => items
            .iter()
            .find_map(|item| item.get("msg").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| Value::Array(items).to_string()),
        value => value.to_string(),
    }
}

fn datastream_cache_key(server: &ServerConfig, workspace_id: &str) -> String {
    let credential = match server.auth_type {
        AuthType::Apikey => server.api_key.trim(),
        AuthType::Userpass => server.username.trim(),
    };

    format!(
        "{:?}|{}|{}|{}",
        server.auth_type,
        normalize_url(&server.url),
        workspace_id.trim(),
        credential
    )
}

fn expanded_datastream_to_summary(item: &Value) -> DatastreamSummary {
    let thing = item.get("monitoringSite");
    let observed_property = item
        .get("observed_property")
        .or_else(|| item.get("observedProperty"));
    let processing_level = item
        .get("processing_level")
        .or_else(|| item.get("processingLevel"));
    let unit = item.get("unit");
    let sensor = item.get("method");

    let thing_id = string_value(item, &["monitoring_site_id", "monitoringSiteId"])
        .or_else(|| thing.and_then(|thing| string_value(thing, &["id", "uid"])))
        .unwrap_or_default();

    DatastreamSummary {
        id: string_value(item, &["id", "uid"]).unwrap_or_default(),
        name: string_value(item, &["name"]).unwrap_or_else(|| "Unnamed datastream".to_string()),
        thing_id,
        thing_name: thing
            .and_then(|value| string_value(value, &["name"]))
            .unwrap_or_default(),
        observed_property_name: observed_property
            .and_then(|value| string_value(value, &["name"]))
            .unwrap_or_default(),
        processing_level_definition: processing_level
            .and_then(|value| string_value(value, &["name"]))
            .unwrap_or_default(),
        unit_name: unit
            .and_then(|value| string_value(value, &["name"]))
            .unwrap_or_default(),
        unit_symbol: unit
            .and_then(|value| string_value(value, &["symbol"]))
            .unwrap_or_default(),
        sampled_medium: string_value(item, &["sampled_medium", "sampledMedium"])
            .unwrap_or_default(),
        sensor_name: sensor
            .and_then(|value| string_value(value, &["name"]))
            .unwrap_or_default(),
        result_type: string_value(item, &["result_type", "resultType"]).unwrap_or_default(),
    }
}

fn expanded_datastream_to_detail(item: &Value) -> DatastreamDetail {
    let thing = item.get("monitoringSite");
    let observed_property = item
        .get("observed_property")
        .or_else(|| item.get("observedProperty"));
    let processing_level = item
        .get("processing_level")
        .or_else(|| item.get("processingLevel"));
    let unit = item.get("unit");
    let sensor = item.get("method");

    DatastreamDetail {
        id: string_value(item, &["id", "uid"]).unwrap_or_default(),
        name: string_value(item, &["name"]).unwrap_or_else(|| "Unnamed datastream".to_string()),
        description: string_value(item, &["description"]).unwrap_or_default(),
        sampled_medium: string_value(item, &["sampled_medium", "sampledMedium"])
            .unwrap_or_default(),
        result_type: string_value(item, &["result_type", "resultType"]).unwrap_or_default(),
        observation_type: string_value(item, &["observation_type", "observationType"])
            .unwrap_or_default(),
        no_data_value: scalar_string_value(item, &["no_data_value", "noDataValue"]),
        aggregation_statistic: string_value(
            item,
            &["aggregation_statistic", "aggregationStatistic"],
        )
        .unwrap_or_default(),
        intended_time_spacing: scalar_string_value(
            item,
            &["intended_time_spacing", "intendedTimeSpacing"],
        ),
        intended_time_spacing_unit: string_value(
            item,
            &["intended_time_spacing_unit", "intendedTimeSpacingUnit"],
        )
        .unwrap_or_default(),
        time_aggregation_interval: scalar_string_value(
            item,
            &["time_aggregation_interval", "timeAggregationInterval"],
        ),
        time_aggregation_interval_unit: string_value(
            item,
            &[
                "time_aggregation_interval_unit",
                "timeAggregationIntervalUnit",
            ],
        )
        .unwrap_or_default(),
        phenomenon_begin_time: scalar_string_value(
            item,
            &["phenomenon_begin_time", "phenomenonBeginTime"],
        ),
        phenomenon_end_time: scalar_string_value(
            item,
            &["phenomenon_end_time", "phenomenonEndTime"],
        ),
        value_count: scalar_string_value(item, &["value_count", "valueCount"]),
        is_private: bool_value(item, &["is_private", "isPrivate"]),
        is_visible: bool_value(item, &["is_visible", "isVisible"]),
        thing: DatastreamThingDetail {
            id: string_value(item, &["monitoring_site_id", "monitoringSiteId"])
                .or_else(|| thing.and_then(|value| string_value(value, &["id", "uid"])))
                .unwrap_or_default(),
            name: thing
                .and_then(|value| string_value(value, &["name"]))
                .unwrap_or_default(),
            description: thing
                .and_then(|value| string_value(value, &["description"]))
                .unwrap_or_default(),
            sampling_feature_code: thing
                .and_then(|value| string_value(value, &["code"]))
                .unwrap_or_default(),
            site_type: thing
                .and_then(|value| string_value(value, &["type"]))
                .unwrap_or_default(),
            sampling_feature_type: thing
                .and_then(|value| {
                    string_value(value, &["sampling_feature_type", "samplingFeatureType"])
                })
                .unwrap_or_default(),
            is_private: thing
                .map(|value| bool_value(value, &["is_private", "isPrivate"]))
                .unwrap_or(false),
            location: DatastreamThingLocationDetail {
                latitude: thing
                    .map(|value| scalar_string_value(value, &["latitude"]))
                    .unwrap_or_default(),
                longitude: thing
                    .map(|value| scalar_string_value(value, &["longitude"]))
                    .unwrap_or_default(),
                elevation_m: thing
                    .map(|value| scalar_string_value(value, &["elevation_m", "elevationM"]))
                    .unwrap_or_default(),
                elevation_datum: thing
                    .and_then(|value| string_value(value, &["elevation_datum", "elevationDatum"]))
                    .unwrap_or_default(),
                admin_area_1: thing
                    .and_then(|value| string_value(value, &["admin_area_1", "adminArea1"]))
                    .unwrap_or_default(),
                admin_area_2: thing
                    .and_then(|value| string_value(value, &["admin_area_2", "adminArea2"]))
                    .unwrap_or_default(),
                country: thing
                    .and_then(|value| string_value(value, &["country"]))
                    .unwrap_or_default(),
            },
        },
        observed_property: DatastreamObservedPropertyDetail {
            id: string_value(item, &["observed_property_id", "observedPropertyId"])
                .or_else(|| observed_property.and_then(|value| string_value(value, &["id", "uid"])))
                .unwrap_or_default(),
            name: observed_property
                .and_then(|value| string_value(value, &["name"]))
                .unwrap_or_default(),
            definition: observed_property
                .and_then(|value| string_value(value, &["definition"]))
                .unwrap_or_default(),
            description: observed_property
                .and_then(|value| string_value(value, &["description"]))
                .unwrap_or_default(),
            property_type: observed_property
                .and_then(|value| string_value(value, &["type"]))
                .unwrap_or_default(),
            code: observed_property
                .and_then(|value| string_value(value, &["code"]))
                .unwrap_or_default(),
        },
        unit: DatastreamUnitDetail {
            id: string_value(item, &["unit_id", "unitId"])
                .or_else(|| unit.and_then(|value| string_value(value, &["id", "uid"])))
                .unwrap_or_default(),
            name: unit
                .and_then(|value| string_value(value, &["name"]))
                .unwrap_or_default(),
            symbol: unit
                .and_then(|value| string_value(value, &["symbol"]))
                .unwrap_or_default(),
            definition: unit
                .and_then(|value| string_value(value, &["definition"]))
                .unwrap_or_default(),
            unit_type: unit
                .and_then(|value| string_value(value, &["type"]))
                .unwrap_or_default(),
        },
        sensor: DatastreamSensorDetail {
            id: string_value(item, &["method_id", "methodId"])
                .or_else(|| sensor.and_then(|value| string_value(value, &["id", "uid"])))
                .unwrap_or_default(),
            name: sensor
                .and_then(|value| string_value(value, &["name"]))
                .unwrap_or_default(),
            description: sensor
                .and_then(|value| string_value(value, &["description"]))
                .unwrap_or_default(),
            manufacturer: sensor
                .and_then(|value| string_value(value, &["sensorModelManufacturer"]))
                .unwrap_or_default(),
            model: sensor
                .and_then(|value| string_value(value, &["sensorModel"]))
                .unwrap_or_default(),
            method_type: sensor
                .and_then(|value| string_value(value, &["type"]))
                .unwrap_or_default(),
            method_code: sensor
                .and_then(|value| string_value(value, &["code"]))
                .unwrap_or_default(),
            method_link: sensor
                .and_then(|value| string_value(value, &["definition"]))
                .unwrap_or_default(),
            encoding_type: sensor
                .and_then(|value| string_value(value, &["encoding_type", "encodingType"]))
                .unwrap_or_default(),
            model_link: sensor
                .and_then(|value| string_value(value, &["sensorModelDefinition"]))
                .unwrap_or_default(),
        },
        processing_level: DatastreamProcessingLevelDetail {
            id: string_value(item, &["processing_level_id", "processingLevelId"])
                .or_else(|| processing_level.and_then(|value| string_value(value, &["id", "uid"])))
                .unwrap_or_default(),
            code: processing_level
                .and_then(|value| string_value(value, &["code"]))
                .unwrap_or_default(),
            definition: processing_level
                .and_then(|value| string_value(value, &["name"]))
                .unwrap_or_default(),
            explanation: processing_level
                .and_then(|value| string_value(value, &["description"]))
                .unwrap_or_default(),
        },
    }
}

fn response_item(payload: &Value) -> Result<&Value, RequestError> {
    payload
        .get("data")
        .filter(|item| item.is_object())
        .ok_or_else(|| {
            RequestError::Other("HydroServer returned an invalid item response.".to_string())
        })
}

// Keep the SDL's persisted/UI models stable while resolving v2's side-loaded
// resources. Monitoring sites and methods fill the existing thing/sensor fields.
fn attach_datastream_relations(items: &mut [Value], payload: &Value) {
    for (id_field, relation, bucket) in [
        ("monitoringSiteId", "monitoringSite", "monitoringSites"),
        (
            "observedPropertyId",
            "observedProperty",
            "observedProperties",
        ),
        ("processingLevelId", "processingLevel", "processingLevels"),
        ("unitId", "unit", "units"),
        ("methodId", "method", "methods"),
    ] {
        let Some(resources) = payload
            .get("included")
            .and_then(|included| included.get(bucket))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let by_id = map_items_by_id(resources);
        for item in items.iter_mut() {
            if let Some(related) = item
                .get(id_field)
                .and_then(Value::as_str)
                .and_then(|id| by_id.get(id))
            {
                let related = related.clone();
                if let Some(object) = item.as_object_mut() {
                    object.insert(relation.to_string(), related);
                }
            }
        }
    }
}

fn map_items_by_id(items: &[Value]) -> HashMap<String, Value> {
    items
        .iter()
        .filter_map(|item| string_value(item, &["id", "uid"]).map(|id| (id, item.clone())))
        .collect()
}

fn string_value(item: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| item.get(*key))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn scalar_string_value(item: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| item.get(*key))
        .and_then(value_to_string)
        .unwrap_or_default()
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn bool_value(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "tests/hydroserver.rs"]
mod tests;
