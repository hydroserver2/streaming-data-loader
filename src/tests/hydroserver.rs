use super::*;
use chrono::TimeZone;

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

fn server_with_workspace(workspace_id: &str) -> ServerConfig {
    ServerConfig {
        auth_type: AuthType::Apikey,
        url: "https://example.com".to_string(),
        api_key: "test-key".to_string(),
        workspace_id: workspace_id.to_string(),
        workspace_name: String::new(),
        ..ServerConfig::default()
    }
}

/// bug_008: When a persisted API key can no longer access the saved
/// workspace (e.g., key rotated to a different workspace), we must surface an
/// error instead of silently picking a different workspace from the list.
#[test]
fn resolve_api_key_workspace_errors_when_saved_workspace_is_unreachable() {
    let server = server_with_workspace("workspace-saved");
    let accessible = vec![
        ("workspace-a".to_string(), "Workspace A".to_string()),
        ("workspace-b".to_string(), "Workspace B".to_string()),
    ];

    let result = resolve_api_key_workspace(&server, &accessible);
    let (field, message) = result.expect_err("should error when saved workspace is missing");
    assert_eq!(field, "api_key");
    assert!(
        message.contains("does not have access"),
        "message should explain the key lost access, got: {message}"
    );
}

#[test]
fn resolve_api_key_workspace_returns_saved_workspace_when_accessible() {
    let server = server_with_workspace("workspace-saved");
    let accessible = vec![
        ("workspace-a".to_string(), "Workspace A".to_string()),
        ("workspace-saved".to_string(), "Saved Workspace".to_string()),
    ];

    let (id, name) = resolve_api_key_workspace(&server, &accessible)
        .expect("should succeed")
        .expect("should return saved workspace");
    assert_eq!(id, "workspace-saved");
    assert_eq!(name, "Saved Workspace");
}

#[test]
fn resolve_api_key_workspace_picks_first_when_none_saved() {
    let server = server_with_workspace("");
    let accessible = vec![
        ("workspace-a".to_string(), "Workspace A".to_string()),
        ("workspace-b".to_string(), "Workspace B".to_string()),
    ];

    let (id, _) = resolve_api_key_workspace(&server, &accessible)
        .expect("should succeed")
        .expect("should return first workspace");
    assert_eq!(
        id, "workspace-a",
        "initial connection defaults to the first accessible workspace"
    );
}

#[test]
fn resolve_api_key_workspace_returns_none_when_no_workspaces_accessible() {
    let server = server_with_workspace("workspace-saved");
    let empty: Vec<(String, String)> = Vec::new();

    let result = resolve_api_key_workspace(&server, &empty).expect("empty list is not an error");
    assert!(
        result.is_none(),
        "no accessible workspaces should produce None (test_connection surfaces the key-level error)"
    );
}

fn userpass_server(url: &str, username: &str) -> ServerConfig {
    ServerConfig {
        auth_type: AuthType::Userpass,
        url: url.to_string(),
        username: username.to_string(),
        password: "secret".to_string(),
        ..Default::default()
    }
}

// Exercise the real HTTP client against v2 response shapes, including servers
// that return fewer items per page than the requested limit.
use axum::{body::to_bytes, extract::Request, response::IntoResponse, routing::any, Json, Router};
use std::{collections::VecDeque, sync::Arc};
use tokio::{net::TcpListener, task::JoinHandle};

#[derive(Debug)]
struct RecordedRequest {
    method: Method,
    url: reqwest::Url,
    headers: reqwest::header::HeaderMap,
    body: Value,
}

struct TestServer {
    url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    task: JoinHandle<()>,
}

impl TestServer {
    async fn spawn(responses: Vec<(u16, Value)>) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let handler = {
            let requests = requests.clone();
            move |request: Request| {
                let requests = requests.clone();
                let responses = responses.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let bytes = to_bytes(body, 1024 * 1024).await.unwrap();
                    requests.lock().unwrap().push(RecordedRequest {
                        method: parts.method,
                        url: reqwest::Url::parse(&format!("http://localhost{}", parts.uri))
                            .unwrap(),
                        headers: parts.headers,
                        body: if bytes.is_empty() {
                            Value::Null
                        } else {
                            serde_json::from_slice(&bytes).unwrap()
                        },
                    });
                    let (status, body) = responses
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or((500, json!({"message": "Unexpected request"})));
                    (StatusCode::from_u16(status).unwrap(), Json(body)).into_response()
                }
            }
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(any(handler));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            requests,
            task,
        }
    }

    fn config(&self) -> ServerConfig {
        ServerConfig {
            url: self.url.clone(),
            ..server_with_workspace("workspace-1")
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn collection(items: Value, offset: u64, total: u64) -> Value {
    json!({"data": items, "meta": {"offset": offset, "limit": 1, "totalCount": total}})
}

fn query(request: &RecordedRequest, name: &str) -> Option<String> {
    request
        .url
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

#[tokio::test]
async fn validates_v2_instance_with_no_public_workspaces() {
    let server = TestServer::spawn(vec![(200, collection(json!([]), 0, 0))]).await;
    let service = HydroServerService::new().unwrap();
    assert!(service.validate_url(&format!("{}/", server.url)).await.ok);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/api/data/workspaces");
    assert_eq!(query(&requests[0], "limit").as_deref(), Some("1"));
    assert!(!requests[0].headers.contains_key("authorization"));
}

#[tokio::test]
async fn rejects_unrecognized_server_responses() {
    for body in [
        json!([]),
        json!({"data": []}),
        json!({"message": "Not found"}),
    ] {
        let server = TestServer::spawn(vec![(200, body)]).await;
        assert!(
            !HydroServerService::new()
                .unwrap()
                .validate_url(&server.url)
                .await
                .ok
        );
    }
}

#[tokio::test]
async fn api_key_connection_reads_all_workspace_pages() {
    let server = TestServer::spawn(vec![
        (
            200,
            collection(json!([{"id": "other", "name": "Other"}]), 0, 2),
        ),
        (
            200,
            collection(
                json!([{"id": "workspace-1", "name": "Test workspace"}]),
                1,
                2,
            ),
        ),
    ])
    .await;
    let response = HydroServerService::new()
        .unwrap()
        .test_connection(&server.config())
        .await;
    assert!(response.ok, "{}", response.message);
    assert_eq!(response.workspace_count, 2);
    assert_eq!(response.workspace_id.as_deref(), Some("workspace-1"));
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request.headers["x-api-key"], "test-key");
        assert!(!request.headers.contains_key("authorization"));
        assert_eq!(query(request, "is_associated").as_deref(), Some("true"));
        assert_eq!(query(request, "offset"), Some(index.to_string()));
        assert_eq!(query(request, "limit").as_deref(), Some("1000"));
        assert_eq!(query(request, "page"), None);
    }
}

#[tokio::test]
async fn username_password_uses_basic_auth_without_session_login() {
    let server = TestServer::spawn(vec![
        (
            200,
            collection(
                json!([{"id": "workspace-1", "name": "Test workspace"}]),
                0,
                1,
            ),
        ),
        (401, json!({"message": "Invalid username or password"})),
    ])
    .await;
    let mut config = userpass_server(&server.url, "user@example.com");
    config.workspace_name = "Test workspace".to_string();
    let service = HydroServerService::new().unwrap();
    assert!(service.test_connection(&config).await.ok);
    config.password = "wrong".to_string();
    let rejected = service.test_connection(&config).await;
    assert!(!rejected.ok);
    assert_eq!(rejected.invalid_field.as_deref(), Some("username"));
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "invalid credentials must not trigger a session login"
    );
    assert_eq!(
        requests[0].headers["authorization"],
        "Basic dXNlckBleGFtcGxlLmNvbTpzZWNyZXQ="
    );
    assert_ne!(
        requests[0].headers["authorization"],
        requests[1].headers["authorization"]
    );
    assert!(requests
        .iter()
        .all(|r| r.method == Method::GET && r.url.path() == "/api/data/workspaces"));
}

fn datastream_item() -> Value {
    json!({
        "id": "ds-1", "name": "Stage", "description": "River stage", "workspaceId": "workspace-1",
        "monitoringSiteId": "site-1", "methodId": "method-1", "observedPropertyId": "op-1",
        "processingLevelId": "pl-1", "unitId": "unit-1", "sampledMedium": "Water",
        "resultType": "Time series", "observationType": "OM_Measurement", "noDataValue": -9999,
        "phenomenonEndTime": "2026-09-24T12:00:00Z", "valueCount": 7, "isVisible": true
    })
}

fn included_resources() -> Value {
    json!({
        "monitoringSites": [
            {"id": "other", "name": "Wrong site"},
            {"id": "site-1", "name": "River", "code": "R1", "type": "Stream", "description": "River site",
             "latitude": 41.5, "longitude": -111.5, "elevation_m": 1400, "elevationDatum": "NAVD88",
             "adminArea1": "Utah", "adminArea2": "Cache", "country": "US", "isPrivate": true}
        ],
        "methods": [{"id": "method-1", "name": "Pressure", "code": "P1", "type": "Instrument",
                     "description": "Pressure transducer", "definition": "https://example.com/method",
                     "sensorModel": "Model A", "sensorModelManufacturer": "Manufacturer",
                     "sensorModelDefinition": "https://example.com/model"}],
        "observedProperties": [{"id": "op-1", "name": "Stage", "type": "Depth", "code": "STAGE", "description": "Water level"}],
        "processingLevels": [{"id": "pl-1", "name": "Raw", "description": "Unprocessed measurements", "code": "0", "definition": "https://example.com/level"}],
        "units": [{"id": "unit-1", "name": "Meter", "symbol": "m", "type": "Length"}]
    })
}

#[tokio::test]
async fn datastream_pages_resolve_included_metadata_and_cache_results() {
    let mut first = collection(json!([datastream_item()]), 0, 2);
    first["included"] = included_resources();
    let mut next_item = datastream_item();
    next_item["id"] = json!("ds-2");
    let mut second = collection(json!([next_item]), 1, 2);
    second["included"] = included_resources();
    second["included"]["monitoringSites"][1]["name"] = json!("Second page site");
    let server = TestServer::spawn(vec![
        (200, first),
        (200, second),
        (200, collection(json!([]), 0, 0)),
    ])
    .await;
    let service = HydroServerService::new().unwrap();
    let config = server.config();
    let items = service.list_datastreams(&config, false).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].thing_id, "site-1");
    assert_eq!(items[0].thing_name, "River");
    assert_eq!(items[0].sensor_name, "Pressure");
    assert_eq!(items[0].observed_property_name, "Stage");
    assert_eq!(items[0].processing_level_definition, "Raw");
    assert_eq!(items[0].unit_name, "Meter");
    assert_eq!(items[0].unit_symbol, "m");
    assert_eq!(items[0].sampled_medium, "Water");
    assert_eq!(items[0].result_type, "Time series");
    assert_eq!(items[1].thing_name, "Second page site");
    assert_eq!(
        service
            .list_datastreams(&config, false)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    assert!(service
        .list_datastreams(&config, true)
        .await
        .unwrap()
        .is_empty());
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests.iter() {
        assert_eq!(request.url.path(), "/api/data/datastreams");
        assert_eq!(
            query(request, "include").as_deref(),
            Some(DATASTREAM_INCLUDE)
        );
        assert_eq!(
            query(request, "workspace_id").as_deref(),
            Some("workspace-1")
        );
        assert_eq!(query(request, "expand_related"), None);
    }
    assert_eq!(query(&requests[1], "offset").as_deref(), Some("1"));
}

#[tokio::test]
async fn datastream_detail_maps_v2_metadata_into_existing_ui_fields() {
    let server = TestServer::spawn(vec![(
        200,
        json!({"data": datastream_item(), "included": included_resources()}),
    )])
    .await;
    let detail = HydroServerService::new()
        .unwrap()
        .get_datastream_detail(&server.config(), "ds-1")
        .await
        .unwrap();
    assert_eq!(detail.id, "ds-1");
    assert_eq!(detail.no_data_value, "-9999");
    assert_eq!(detail.value_count, "7");
    assert!(detail.is_visible);
    assert_eq!(detail.thing.id, "site-1");
    assert_eq!(detail.thing.name, "River");
    assert_eq!(detail.thing.sampling_feature_code, "R1");
    assert_eq!(detail.thing.site_type, "Stream");
    assert!(detail.thing.is_private);
    assert_eq!(detail.thing.location.latitude, "41.5");
    assert_eq!(detail.thing.location.longitude, "-111.5");
    assert_eq!(detail.thing.location.elevation_m, "1400");
    assert_eq!(detail.thing.location.elevation_datum, "NAVD88");
    assert_eq!(detail.thing.location.admin_area_1, "Utah");
    assert_eq!(detail.thing.location.admin_area_2, "Cache");
    assert_eq!(detail.thing.location.country, "US");
    assert_eq!(detail.sensor.id, "method-1");
    assert_eq!(detail.sensor.name, "Pressure");
    assert_eq!(detail.sensor.manufacturer, "Manufacturer");
    assert_eq!(detail.sensor.model, "Model A");
    assert_eq!(detail.sensor.method_type, "Instrument");
    assert_eq!(detail.sensor.method_code, "P1");
    assert_eq!(detail.sensor.method_link, "https://example.com/method");
    assert_eq!(detail.sensor.model_link, "https://example.com/model");
    assert_eq!(detail.processing_level.definition, "Raw");
    assert_eq!(
        detail.processing_level.explanation,
        "Unprocessed measurements"
    );
    assert_eq!(detail.unit.symbol, "m");
    assert_eq!(detail.observed_property.code, "STAGE");
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests[0].url.path(), "/api/data/datastreams/ds-1");
    assert_eq!(
        query(&requests[0], "include").as_deref(),
        Some(DATASTREAM_INCLUDE)
    );
}

#[tokio::test]
async fn malformed_collections_fail_instead_of_silently_hiding_datastreams() {
    for payload in [json!([]), json!({"data": []}), collection(json!([]), 0, 1)] {
        let server = TestServer::spawn(vec![(200, payload)]).await;
        assert!(HydroServerService::new()
            .unwrap()
            .list_datastreams(&server.config(), false)
            .await
            .is_err());
    }
}

#[test]
fn missing_monitoring_site_does_not_use_datastream_location_fields() {
    let item = json!({
        "id": "ds-1", "monitoringSiteId": "site-1",
        "latitude": 41.5, "longitude": -111.5, "elevation_m": 1400,
        "elevationDatum": "NAVD88", "adminArea1": "Utah",
        "adminArea2": "Cache", "country": "US"
    });
    let detail = expanded_datastream_to_detail(&item);
    assert_eq!(detail.thing.id, "site-1");
    let location = detail.thing.location;
    assert!(location.latitude.is_empty());
    assert!(location.longitude.is_empty());
    assert!(location.elevation_m.is_empty());
    assert!(location.elevation_datum.is_empty());
    assert!(location.admin_area_1.is_empty());
    assert!(location.admin_area_2.is_empty());
    assert!(location.country.is_empty());
}

#[tokio::test]
async fn reads_conflict_watermark_from_item_envelope() {
    let server = TestServer::spawn(vec![
        (
            200,
            json!({"data": {"phenomenonEndTime": "2026-09-24T12:00:00Z"}}),
        ),
        (200, json!({"data": {"phenomenonEndTime": null}})),
        (200, json!({})),
    ])
    .await;
    let service = HydroServerService::new().unwrap();
    assert_eq!(
        service
            .fetch_phenomenon_end_time(&server.config(), "ds-1")
            .await
            .unwrap(),
        Some(Utc.with_ymd_and_hms(2026, 9, 24, 12, 0, 0).unwrap())
    );
    assert_eq!(
        service
            .fetch_phenomenon_end_time(&server.config(), "ds-1")
            .await
            .unwrap(),
        None
    );
    assert!(service
        .fetch_phenomenon_end_time(&server.config(), "ds-1")
        .await
        .is_err());
}

#[tokio::test]
async fn uploads_to_v2_bulk_endpoint_and_preserves_insert_conflicts() {
    let server = TestServer::spawn(vec![
        (201, Value::Null),
        (
            409,
            json!({"message": "Duplicate phenomenonTime found on this datastream."}),
        ),
    ])
    .await;
    let service = HydroServerService::new().unwrap();
    let rows = vec![
        ObservationPayloadRow {
            phenomenon_time: Utc.with_ymd_and_hms(2026, 9, 24, 12, 0, 0).unwrap(),
            result: json!(2.41),
        },
        ObservationPayloadRow {
            phenomenon_time: Utc.with_ymd_and_hms(2026, 9, 24, 12, 5, 0).unwrap(),
            result: Value::Null,
        },
    ];
    service
        .post_observations_batch(&server.config(), "ds-1", &[])
        .await
        .unwrap();
    assert!(server.requests.lock().unwrap().is_empty());
    service
        .post_observations_batch(&server.config(), "ds-1", &rows)
        .await
        .unwrap();
    let conflict = service
        .post_observations_batch(&server.config(), "ds-1", &rows)
        .await
        .unwrap_err();
    assert!(conflict.is_conflict());
    assert!(conflict.to_string().contains("Duplicate phenomenonTime"));
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, Method::POST);
    assert_eq!(requests[0].url.path(), "/api/data/observations/bulk-create");
    assert_eq!(query(&requests[0], "mode").as_deref(), Some("insert"));
    assert_eq!(requests[0].headers["x-api-key"], "test-key");
    assert_eq!(
        requests[0].body,
        json!({
            "datastreamId": "ds-1", "fields": ["phenomenonTime", "result"],
            "data": [["2026-09-24T12:00:00+00:00", 2.41], ["2026-09-24T12:05:00+00:00", null]]
        })
    );
    assert_eq!(requests[0].body, requests[1].body);
}
