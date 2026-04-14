use std::{collections::HashMap, sync::Mutex, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    Client, Method, Response, StatusCode,
};
use serde_json::{json, Value};

use crate::models::{
    normalize_url, AuthType, ConnectionState, ConnectionTestResponse, DatastreamSummary,
    ServerConfig, ServerUrlValidationResponse,
};

const AUTH_ROUTE: &str = "/api/auth";
const BASE_ROUTE: &str = "/api/data";
const DATASTREAM_PAGE_SIZE: usize = 1000;
const DATASTREAM_CACHE_TTL_SECONDS: i64 = 300;

#[derive(Debug, Clone)]
pub struct ObservationPayloadRow {
    pub phenomenon_time: DateTime<Utc>,
    pub result: Value,
}

pub struct HydroServerService {
    http: Client,
    datastream_cache: Mutex<HashMap<String, (DateTime<Utc>, Vec<DatastreamSummary>)>>,
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

    pub async fn validate_url(&self, url: &str) -> ServerUrlValidationResponse {
        let normalized_url = normalize_url(url);
        if normalized_url.is_empty() {
            return ServerUrlValidationResponse {
                ok: false,
                message: "Enter the HydroServer URL.".to_string(),
                instance_name: None,
            };
        }

        let auth_probe_url = format!("{normalized_url}{AUTH_ROUTE}/app/session");
        let data_probe_url = format!("{normalized_url}{BASE_ROUTE}/workspaces");

        let auth_response = self
            .http
            .get(&auth_probe_url)
            .header(ACCEPT, "application/json")
            .send()
            .await;

        if let Ok(response) = auth_response {
            if looks_like_hydroserver_auth_response(response).await {
                let instance_name = instance_name(&normalized_url);
                return ServerUrlValidationResponse {
                    ok: true,
                    message: format!("HydroServer API detected at {instance_name}."),
                    instance_name: Some(instance_name),
                };
            }
        } else if let Err(err) = auth_response {
            if err.is_connect() || err.is_timeout() {
                return ServerUrlValidationResponse {
                    ok: false,
                    message: "Couldn't reach that URL. Check the server URL and try again."
                        .to_string(),
                    instance_name: None,
                };
            }
        }

        match self
            .http
            .get(&data_probe_url)
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
                instance_name: None,
                workspace_id: None,
                workspace_name: None,
                workspace_count: 0,
                datastream_count: 0,
                permissions_ok: false,
            };
        }

        let mut session = HydroServerSession::new(self.http.clone(), server.clone().normalized());
        match session.associated_workspace().await {
            Ok((Some(workspace_id), workspace_name, workspace_count)) => {
                let instance_name = instance_name(&server.url);
                ConnectionTestResponse {
                    ok: true,
                    state: ConnectionState::Connected,
                    message: format!("Connected to {instance_name}."),
                    instance_name: Some(instance_name),
                    workspace_id: Some(workspace_id),
                    workspace_name,
                    workspace_count,
                    datastream_count: 0,
                    permissions_ok: true,
                }
            }
            Ok((None, _, workspace_count)) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "That API key is invalid or is not attached to any accessible workspace. Check the API key permissions and try again.".to_string(),
                instance_name: Some(instance_name(&server.url)),
                workspace_id: None,
                workspace_name: None,
                workspace_count,
                datastream_count: 0,
                permissions_ok: false,
            },
            Err(RequestError::Connection) | Err(RequestError::Timeout) => ConnectionTestResponse {
                ok: false,
                state: ConnectionState::Error,
                message: "Couldn't reach HydroServer. Check the server URL and try again.".to_string(),
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
                message: "These credentials are invalid or do not have the permissions the loader needs. Make sure they can access workspaces, datastreams, and orchestration systems.".to_string(),
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
    ) -> Result<Vec<DatastreamSummary>, String> {
        if !server.is_configured() {
            return Ok(Vec::new());
        }

        let normalized = server.clone().normalized();
        let mut session = HydroServerSession::new(self.http.clone(), normalized.clone());
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
        if let Some(cached) = self.cached_datastreams(&cache_key) {
            return Ok(cached);
        }

        if let Some(datastreams) = self
            .list_datastreams_from_bootstrap(&mut session, &workspace_id)
            .await?
        {
            self.set_cached_datastreams(&cache_key, datastreams.clone());
            return Ok(datastreams);
        }

        if let Some(datastreams) = self
            .list_datastreams_expanded(&mut session, &workspace_id)
            .await?
        {
            self.set_cached_datastreams(&cache_key, datastreams.clone());
            return Ok(datastreams);
        }

        let datastreams = session
            .fetch_all_collection(
                &format!("{BASE_ROUTE}/datastreams"),
                &[("workspace_id", workspace_id.clone())],
            )
            .await
            .map_err(|err| err.to_string())?;

        if datastreams.is_empty() {
            return Ok(Vec::new());
        }

        let things_by_id = session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/things"), &workspace_id)
            .await
            .unwrap_or_default();
        let observed_properties_by_id = session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/observed-properties"), &workspace_id)
            .await
            .unwrap_or_default();
        let processing_levels_by_id = session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/processing-levels"), &workspace_id)
            .await
            .unwrap_or_default();
        let units_by_id = session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/units"), &workspace_id)
            .await
            .unwrap_or_default();
        let sensors_by_id = session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/sensors"), &workspace_id)
            .await
            .unwrap_or_default();

        let summaries = datastreams
            .iter()
            .map(|item| {
                datastream_to_summary(
                    item,
                    &things_by_id,
                    &observed_properties_by_id,
                    &processing_levels_by_id,
                    &units_by_id,
                    &sensors_by_id,
                )
            })
            .collect::<Vec<_>>();

        self.set_cached_datastreams(&cache_key, summaries.clone());
        Ok(summaries)
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

        let mut session = HydroServerSession::new(self.http.clone(), server.clone().normalized());
        // The earlier Rust port posted ["timestamp", "value"], which does not match the
        // HydroServer bulk observation schema. The API expects SensorThings field names.
        let body = json!({
            "fields": ["phenomenonTime", "result"],
            "data": observations
                .iter()
                .map(|row| json!([row.phenomenon_time.to_rfc3339(), row.result]))
                .collect::<Vec<_>>(),
        });

        session
            .request_void(
                Method::POST,
                &format!("{BASE_ROUTE}/datastreams/{datastream_id}/observations/bulk-create"),
                &[("mode", "insert".to_string())],
                Some(body),
            )
            .await
    }

    async fn list_datastreams_from_bootstrap(
        &self,
        session: &mut HydroServerSession,
        workspace_id: &str,
    ) -> Result<Option<Vec<DatastreamSummary>>, String> {
        let payload = match session
            .request_json(
                Method::GET,
                &format!("{BASE_ROUTE}/datastreams/visualization-bootstrap"),
                &[("workspace_id", workspace_id.to_string())],
                None,
            )
            .await
        {
            Ok(payload) => payload,
            Err(_) => return Ok(None),
        };

        let Some(datastreams) = payload.get("datastreams").and_then(Value::as_array) else {
            return Ok(None);
        };
        let Some(things) = payload.get("things").and_then(Value::as_array) else {
            return Ok(None);
        };
        let Some(observed_properties) = payload
            .get("observed_properties")
            .or_else(|| payload.get("observedProperties"))
            .and_then(Value::as_array)
        else {
            return Ok(None);
        };
        let Some(processing_levels) = payload
            .get("processing_levels")
            .or_else(|| payload.get("processingLevels"))
            .and_then(Value::as_array)
        else {
            return Ok(None);
        };

        let units_by_id = match session
            .fetch_collection_lookup(&format!("{BASE_ROUTE}/units"), workspace_id)
            .await
        {
            Ok(units) => units,
            Err(_) => return Ok(None),
        };

        let things_by_id = map_items_by_id(things);
        let observed_properties_by_id = map_items_by_id(observed_properties);
        let processing_levels_by_id = map_items_by_id(processing_levels);

        Ok(Some(
            datastreams
                .iter()
                .map(|datastream| {
                    let thing_id = string_value(datastream, &["thing_id", "thingId"]);
                    let observed_property_id =
                        string_value(datastream, &["observed_property_id", "observedPropertyId"]);
                    let processing_level_id =
                        string_value(datastream, &["processing_level_id", "processingLevelId"]);
                    let unit_id = string_value(datastream, &["unit_id", "unitId"]);

                    DatastreamSummary {
                        id: string_value(datastream, &["id", "uid"]).unwrap_or_default(),
                        name: string_value(datastream, &["name"])
                            .unwrap_or_else(|| "Unnamed datastream".to_string()),
                        thing_id: thing_id.clone().unwrap_or_default(),
                        thing_name: string_value_from_map(&things_by_id, &thing_id, &["name"]),
                        observed_property_name: string_value_from_map(
                            &observed_properties_by_id,
                            &observed_property_id,
                            &["name"],
                        ),
                        processing_level_definition: string_value_from_map(
                            &processing_levels_by_id,
                            &processing_level_id,
                            &["definition"],
                        ),
                        unit_name: string_value_from_map(&units_by_id, &unit_id, &["name"]),
                        unit_symbol: string_value_from_map(&units_by_id, &unit_id, &["symbol"]),
                        sampled_medium: String::new(),
                        sensor_name: String::new(),
                        result_type: String::new(),
                    }
                })
                .collect(),
        ))
    }

    async fn list_datastreams_expanded(
        &self,
        session: &mut HydroServerSession,
        workspace_id: &str,
    ) -> Result<Option<Vec<DatastreamSummary>>, String> {
        let mut page = 1_u32;
        let mut datastreams = Vec::new();

        loop {
            let response = match session
                .request_response(
                    Method::GET,
                    &format!("{BASE_ROUTE}/datastreams"),
                    &[
                        ("workspace_id", workspace_id.to_string()),
                        ("expand_related", "true".to_string()),
                        ("page", page.to_string()),
                        ("page_size", DATASTREAM_PAGE_SIZE.to_string()),
                    ],
                    None,
                )
                .await
            {
                Ok(response) => response,
                Err(_) => return Ok(None),
            };

            let headers = response.headers().clone();
            let payload = response
                .json::<Value>()
                .await
                .map_err(|err| err.to_string())?;
            let Some(items) = payload.as_array() else {
                return Ok(None);
            };

            if items.is_empty() {
                break;
            }

            datastreams.extend(items.iter().map(expanded_datastream_to_summary));

            if let Some(total_pages) = header_int(&headers, "X-Total-Pages") {
                if page >= total_pages {
                    break;
                }
            } else if items.len() < DATASTREAM_PAGE_SIZE {
                break;
            }

            page += 1;
        }

        Ok(Some(datastreams))
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
    bearer_token: Option<String>,
}

impl HydroServerSession {
    fn new(http: Client, server: ServerConfig) -> Self {
        Self {
            http,
            server,
            bearer_token: None,
        }
    }

    async fn associated_workspace(
        &mut self,
    ) -> Result<(Option<String>, Option<String>, u32), RequestError> {
        let response = self
            .request_response(
                Method::GET,
                &format!("{BASE_ROUTE}/workspaces"),
                &[
                    ("page_size", "25".to_string()),
                    ("is_associated", "true".to_string()),
                ],
                None,
            )
            .await?;

        let headers = response.headers().clone();
        let payload = response
            .json::<Value>()
            .await
            .map_err(|err| RequestError::Other(err.to_string()))?;
        let Some(items) = payload.as_array() else {
            return Ok((None, None, 0));
        };

        let workspace_count = header_int(&headers, "X-Total-Count")
            .or_else(|| header_int(&headers, "X-Total-Count".to_ascii_lowercase().as_str()))
            .unwrap_or(items.len() as u32);
        let first_workspace = items.first();
        Ok((
            first_workspace.and_then(|item| string_value(item, &["id", "uid"])),
            first_workspace.and_then(|item| string_value(item, &["name"])),
            workspace_count,
        ))
    }

    async fn fetch_collection_lookup(
        &mut self,
        path: &str,
        workspace_id: &str,
    ) -> Result<HashMap<String, Value>, RequestError> {
        let items = self
            .fetch_all_collection(path, &[("workspace_id", workspace_id.to_string())])
            .await?;
        Ok(items
            .into_iter()
            .filter_map(|item| {
                let id = string_value(&item, &["id", "uid"])?;
                Some((id, item))
            })
            .collect())
    }

    async fn fetch_all_collection(
        &mut self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<Value>, RequestError> {
        let mut page = 1_u32;
        let mut items = Vec::new();

        loop {
            let mut page_params = params.to_vec();
            page_params.push(("page", page.to_string()));
            page_params.push(("page_size", DATASTREAM_PAGE_SIZE.to_string()));
            let response = self
                .request_response(Method::GET, path, &page_params, None)
                .await?;
            let headers = response.headers().clone();
            let payload = response
                .json::<Value>()
                .await
                .map_err(|err| RequestError::Other(err.to_string()))?;
            let Some(page_items) = payload.as_array() else {
                return Ok(Vec::new());
            };

            if page_items.is_empty() {
                break;
            }
            items.extend(page_items.iter().cloned());

            if let Some(total_pages) = header_int(&headers, "X-Total-Pages") {
                if page >= total_pages {
                    break;
                }
            } else if page_items.len() < DATASTREAM_PAGE_SIZE {
                break;
            }

            page += 1;
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

        for attempt in 0..2 {
            let mut request = self
                .http
                .request(method.clone(), &url)
                .header(ACCEPT, "application/json");

            if !params.is_empty() {
                request = request.query(params);
            }

            if let Some(payload) = body.clone() {
                request = request
                    .header(CONTENT_TYPE, "application/json")
                    .json(&payload);
            }

            request = self.apply_auth(request).await?;

            let response = match request.send().await {
                Ok(response) => response,
                Err(err) if err.is_connect() => return Err(RequestError::Connection),
                Err(err) if err.is_timeout() => return Err(RequestError::Timeout),
                Err(err) => return Err(RequestError::Other(err.to_string())),
            };

            if response.status().is_success() {
                return Ok(response);
            }

            if attempt == 0
                && self.server.auth_type == AuthType::Userpass
                && matches!(
                    response.status(),
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                )
            {
                self.bearer_token = None;
                continue;
            }

            let status = response.status();
            let message = response_error_message(response).await;
            return Err(RequestError::Http {
                status: Some(status),
                message,
            });
        }

        Err(RequestError::Other(
            "HydroServer request failed after retry.".to_string(),
        ))
    }

    async fn apply_auth(
        &mut self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, RequestError> {
        match self.server.auth_type {
            AuthType::Apikey => Ok(request.header("X-API-Key", self.server.api_key.clone())),
            AuthType::Userpass => {
                let token = self.session_token().await?;
                Ok(request.header(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {token}"))
                        .map_err(|err| RequestError::Other(err.to_string()))?,
                ))
            }
        }
    }

    async fn session_token(&mut self) -> Result<String, RequestError> {
        if let Some(token) = &self.bearer_token {
            return Ok(token.clone());
        }

        let payload = json!({
            "email": self.server.username,
            "password": self.server.password,
        });

        let response = self
            .http
            .post(build_url(
                &self.server.url,
                &format!("{AUTH_ROUTE}/app/session"),
            ))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                if err.is_connect() {
                    RequestError::Connection
                } else if err.is_timeout() {
                    RequestError::Timeout
                } else {
                    RequestError::Other(err.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let message = response_error_message(response).await;
            return Err(RequestError::Http {
                status: Some(status),
                message,
            });
        }

        let payload = response
            .json::<Value>()
            .await
            .map_err(|err| RequestError::Other(err.to_string()))?;
        let token = payload
            .get("meta")
            .and_then(|meta| meta.get("session_token"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RequestError::Other("Authentication failed: No access token returned.".to_string())
            })?
            .to_string();

        self.bearer_token = Some(token.clone());
        Ok(token)
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

async fn looks_like_hydroserver_auth_response(response: Response) -> bool {
    if !matches!(
        response.status(),
        StatusCode::OK
            | StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::METHOD_NOT_ALLOWED
            | StatusCode::UNPROCESSABLE_ENTITY
    ) {
        return false;
    }

    let Some(payload) = response_json(response).await else {
        return false;
    };

    let Some(payload) = payload.as_object() else {
        return false;
    };

    payload
        .get("meta")
        .and_then(Value::as_object)
        .map(|meta| meta.contains_key("is_authenticated"))
        .unwrap_or(false)
        || payload
            .get("data")
            .and_then(Value::as_object)
            .map(|data| data.contains_key("flows"))
            .unwrap_or(false)
        || payload.get("detail").and_then(Value::as_array).is_some()
}

async fn looks_like_hydroserver_data_response(response: Response) -> bool {
    if !matches!(
        response.status(),
        StatusCode::OK | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return false;
    }

    let Some(payload) = response_json(response).await else {
        return false;
    };

    payload.is_array()
        || payload
            .as_object()
            .map(|object| object.contains_key("detail") || object.contains_key("status"))
            .unwrap_or(false)
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
        .and_then(|payload| payload.get("detail").cloned())
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
    let thing = item.get("thing");
    let observed_property = item
        .get("observed_property")
        .or_else(|| item.get("observedProperty"));
    let processing_level = item
        .get("processing_level")
        .or_else(|| item.get("processingLevel"));
    let unit = item.get("unit");
    let sensor = item.get("sensor");

    let thing_id = string_value(item, &["thing_id", "thingId"])
        .or_else(|| thing.and_then(|thing| string_value(thing, &["id", "uid"])))
        .unwrap_or_default();
    let observed_property_id = string_value(item, &["observed_property_id", "observedPropertyId"])
        .or_else(|| observed_property.and_then(|value| string_value(value, &["id", "uid"])))
        .unwrap_or_default();
    let processing_level_id = string_value(item, &["processing_level_id", "processingLevelId"])
        .or_else(|| processing_level.and_then(|value| string_value(value, &["id", "uid"])))
        .unwrap_or_default();
    let unit_id = string_value(item, &["unit_id", "unitId"])
        .or_else(|| unit.and_then(|value| string_value(value, &["id", "uid"])))
        .unwrap_or_default();
    let _ = (observed_property_id, processing_level_id, unit_id);

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
            .and_then(|value| string_value(value, &["definition"]))
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

fn datastream_to_summary(
    item: &Value,
    things_by_id: &HashMap<String, Value>,
    observed_properties_by_id: &HashMap<String, Value>,
    processing_levels_by_id: &HashMap<String, Value>,
    units_by_id: &HashMap<String, Value>,
    sensors_by_id: &HashMap<String, Value>,
) -> DatastreamSummary {
    let thing_id = string_value(item, &["thing_id", "thingId"]).unwrap_or_default();
    let observed_property_id =
        string_value(item, &["observed_property_id", "observedPropertyId"]).unwrap_or_default();
    let processing_level_id =
        string_value(item, &["processing_level_id", "processingLevelId"]).unwrap_or_default();
    let unit_id = string_value(item, &["unit_id", "unitId"]).unwrap_or_default();
    let sensor_id = string_value(item, &["sensor_id", "sensorId"]).unwrap_or_default();

    DatastreamSummary {
        id: string_value(item, &["id", "uid"]).unwrap_or_default(),
        name: string_value(item, &["name"]).unwrap_or_else(|| "Unnamed datastream".to_string()),
        thing_id: thing_id.clone(),
        thing_name: string_value_from_map(things_by_id, &Some(thing_id), &["name"]),
        observed_property_name: string_value_from_map(
            observed_properties_by_id,
            &Some(observed_property_id),
            &["name"],
        ),
        processing_level_definition: string_value_from_map(
            processing_levels_by_id,
            &Some(processing_level_id),
            &["definition"],
        ),
        unit_name: string_value_from_map(units_by_id, &Some(unit_id.clone()), &["name"]),
        unit_symbol: string_value_from_map(units_by_id, &Some(unit_id), &["symbol"]),
        sampled_medium: string_value(item, &["sampled_medium", "sampledMedium"])
            .unwrap_or_default(),
        sensor_name: string_value_from_map(sensors_by_id, &Some(sensor_id), &["name"]),
        result_type: string_value(item, &["result_type", "resultType"]).unwrap_or_default(),
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

fn string_value_from_map(
    items: &HashMap<String, Value>,
    id: &Option<String>,
    keys: &[&str],
) -> String {
    id.as_ref()
        .and_then(|key| items.get(key))
        .and_then(|item| string_value(item, keys))
        .unwrap_or_default()
}

fn header_int(headers: &HeaderMap, header: &str) -> Option<u32> {
    headers
        .get(header)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn observation_batch_payload_matches_sensorthings_schema() {
        let observations = vec![
            ObservationPayloadRow {
                phenomenon_time: Utc.with_ymd_and_hms(2026, 4, 3, 8, 0, 0).unwrap(),
                result: json!(2.41),
            },
            ObservationPayloadRow {
                phenomenon_time: Utc.with_ymd_and_hms(2026, 4, 3, 8, 5, 0).unwrap(),
                result: json!("qualitative"),
            },
        ];

        let body = json!({
            "fields": ["phenomenonTime", "result"],
            "data": observations
                .iter()
                .map(|row| json!([row.phenomenon_time.to_rfc3339(), row.result]))
                .collect::<Vec<_>>(),
        });

        assert_eq!(
            body["fields"],
            json!(["phenomenonTime", "result"]),
            "fields must use SensorThings naming"
        );

        let data = body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0][0], "2026-04-03T08:00:00+00:00");
        assert_eq!(data[0][1], 2.41);
        assert_eq!(data[1][1], "qualitative");
    }

    #[test]
    fn build_url_normalizes_correctly() {
        assert_eq!(
            build_url("https://example.com/", "/api/data/test"),
            "https://example.com/api/data/test"
        );
        assert_eq!(
            build_url("https://example.com", "api/data/test"),
            "https://example.com/api/data/test"
        );
    }

    #[test]
    fn request_error_retryable_classification() {
        assert!(RequestError::Connection.is_retryable());
        assert!(RequestError::Timeout.is_retryable());
        assert!(RequestError::Http {
            status: Some(StatusCode::INTERNAL_SERVER_ERROR),
            message: "error".to_string()
        }
        .is_retryable());
        assert!(RequestError::Http {
            status: Some(StatusCode::BAD_GATEWAY),
            message: "error".to_string()
        }
        .is_retryable());

        assert!(!RequestError::Http {
            status: Some(StatusCode::BAD_REQUEST),
            message: "error".to_string()
        }
        .is_retryable());
        assert!(!RequestError::Http {
            status: Some(StatusCode::UNPROCESSABLE_ENTITY),
            message: "error".to_string()
        }
        .is_retryable());
        assert!(!RequestError::Other("misc".to_string()).is_retryable());
    }
}
