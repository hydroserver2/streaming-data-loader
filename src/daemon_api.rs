use std::{convert::Infallible, fs, io, net::SocketAddr, path::PathBuf};

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use rand::RngCore;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};
use tokio_stream::{wrappers::WatchStream, StreamExt};
use tower_http::cors::{Any, CorsLayer};

use crate::{
    daemon_state::DaemonState,
    models::{ActionResponse, DaemonConnectionInfo, JobUpsertRequest, ServerConfig},
    service_paths::daemon_endpoint_path,
};

#[derive(Clone)]
struct ApiState {
    daemon: DaemonState,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedDaemonEndpoint {
    base_url: String,
    token: String,
    pid: u32,
}

#[derive(Debug, Deserialize)]
struct AccessTokenQuery {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct UrlPayload {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ServerPayload {
    server: ServerConfig,
}

#[derive(Debug, Deserialize)]
struct JobIdPayload {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct JobPayload {
    payload: JobUpsertRequest,
}

#[derive(Debug, Deserialize)]
struct UpdateJobPayload {
    job_id: String,
    payload: JobUpsertRequest,
}

#[derive(Debug, Deserialize)]
struct DatastreamPayload {
    datastream_id: String,
}

#[derive(Debug, Deserialize)]
struct DatastreamsPayload {
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize)]
struct CsvPreviewPayload {
    path: String,
    rows: Option<usize>,
}

pub struct DaemonApiServer {
    endpoint_path: PathBuf,
    join_handle: JoinHandle<()>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    token: String,
}

impl DaemonApiServer {
    pub async fn start(daemon: DaemonState, config_dir: PathBuf) -> Result<Self, String> {
        let token = generate_token();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|err| err.to_string())?;
        let address = listener.local_addr().map_err(|err| err.to_string())?;
        let base_url = format!("http://{}", format_socket_addr(address));
        let endpoint_path = daemon_endpoint_path(&config_dir);

        persist_endpoint(
            &endpoint_path,
            &PersistedDaemonEndpoint {
                base_url: base_url.clone(),
                token: token.clone(),
                pid: std::process::id(),
            },
        )?;

        let app_state = ApiState {
            daemon,
            token: token.clone(),
        };
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(Any);
        let router = Router::new()
            .route("/api/commands/ping", post(ping))
            .route("/api/commands/bootstrap", post(bootstrap))
            .route("/api/commands/get-health", post(get_health))
            .route("/api/commands/get-config", post(get_config))
            .route("/api/commands/get-jobs", post(get_jobs))
            .route("/api/commands/get-job", post(get_job))
            .route("/api/commands/get-job-logs", post(get_job_logs))
            .route(
                "/api/commands/update-server-config",
                post(update_server_config),
            )
            .route(
                "/api/commands/clear-server-config",
                post(clear_server_config),
            )
            .route("/api/commands/test-connection", post(test_connection))
            .route(
                "/api/commands/validate-server-url",
                post(validate_server_url),
            )
            .route("/api/commands/create-job", post(create_job))
            .route("/api/commands/update-job", post(update_job))
            .route("/api/commands/delete-job", post(delete_job))
            .route("/api/commands/run-job-now", post(run_job_now))
            .route("/api/commands/enable-job", post(enable_job))
            .route("/api/commands/disable-job", post(disable_job))
            .route("/api/commands/get-datastreams", post(get_datastreams))
            .route(
                "/api/commands/get-datastream-detail",
                post(get_datastream_detail),
            )
            .route("/api/commands/get-csv-preview", post(get_csv_preview))
            .route("/api/status", get(status_stream))
            .layer(cors)
            .with_state(app_state);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let join_handle = tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });

            if let Err(error) = server.await {
                tracing::error!(error = %error, "daemon API server stopped unexpectedly");
            }
        });

        Ok(Self {
            endpoint_path,
            join_handle,
            shutdown_tx: Some(shutdown_tx),
            token,
        })
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.join_handle.await;
        remove_endpoint_if_current(&self.endpoint_path, &self.token);
    }
}

#[derive(Debug)]
pub enum ConnectionReadError {
    MissingEndpoint,
    Incomplete,
    Fatal(String),
}

pub fn read_connection_info(
    config_dir: PathBuf,
) -> Result<DaemonConnectionInfo, ConnectionReadError> {
    let endpoint_path = daemon_endpoint_path(&config_dir);
    let payload = match fs::read_to_string(&endpoint_path) {
        Ok(payload) => payload,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(ConnectionReadError::MissingEndpoint);
        }
        Err(err) => {
            return Err(ConnectionReadError::Fatal(format!(
                "Couldn't read the daemon endpoint file at {}: {err}",
                endpoint_path.display()
            )));
        }
    };

    let endpoint: PersistedDaemonEndpoint = match serde_json::from_str(&payload) {
        Ok(endpoint) => endpoint,
        Err(_) => return Err(ConnectionReadError::Incomplete),
    };

    Ok(DaemonConnectionInfo {
        base_url: endpoint.base_url,
        token: endpoint.token,
    })
}

fn persist_endpoint(path: &PathBuf, endpoint: &PersistedDaemonEndpoint) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let payload = serde_json::to_vec_pretty(endpoint).map_err(|err| err.to_string())?;

    let tmp_path = path.with_extension(format!("json.tmp.{}", std::process::id()));
    fs::write(&tmp_path, &payload).map_err(|err| {
        format!(
            "Couldn't stage daemon endpoint file at {}: {err}",
            tmp_path.display()
        )
    })?;

    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "Couldn't publish daemon endpoint file at {}: {err}",
            path.display()
        ));
    }

    Ok(())
}

fn remove_endpoint_if_current(path: &PathBuf, token: &str) {
    let Ok(payload) = fs::read_to_string(path) else {
        return;
    };
    let Ok(endpoint) = serde_json::from_str::<PersistedDaemonEndpoint>(&payload) else {
        return;
    };
    if endpoint.token == token {
        let _ = fs::remove_file(path);
    }
}

fn format_socket_addr(address: SocketAddr) -> String {
    match address {
        SocketAddr::V4(v4) => v4.to_string(),
        SocketAddr::V6(v6) => format!("[{}]:{}", v6.ip(), v6.port()),
    }
}

fn generate_token() -> String {
    let mut bytes = [0_u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn authorize(headers: &HeaderMap, token: &str) -> Option<Response> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Some((StatusCode::UNAUTHORIZED, "Missing bearer token.").into_response());
    };
    let Ok(value) = value.to_str() else {
        return Some((StatusCode::UNAUTHORIZED, "Invalid bearer token.").into_response());
    };
    let expected = format!("Bearer {token}");
    if value != expected {
        return Some((StatusCode::UNAUTHORIZED, "Invalid bearer token.").into_response());
    }
    None
}

fn command_error(error: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "detail": error })),
    )
        .into_response()
}

fn parse_json_payload<T: DeserializeOwned>(
    payload: Result<Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T, String> {
    payload
        .map(|Json(value)| value)
        .map_err(|rejection| rejection.to_string())
}

async fn ping(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    Json(ActionResponse {
        ok: true,
        message: "pong".to_string(),
    })
    .into_response()
}

async fn bootstrap(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    match state.daemon.bootstrap() {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_health(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    match state.daemon.health() {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_config(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    match state.daemon.config() {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_jobs(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    match state.daemon.jobs() {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.get_job(&payload.job_id) {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_job_logs(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.get_job_logs(&payload.job_id) {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn update_server_config(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<ServerPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.update_server_config(payload.server).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn clear_server_config(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }

    match state.daemon.clear_server_config().await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn test_connection(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<ServerPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.test_connection(payload.server).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn validate_server_url(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<UrlPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.validate_server_url(payload.url).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn create_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.create_job(payload.payload).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn update_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateJobPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state
        .daemon
        .update_job(&payload.job_id, payload.payload)
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn delete_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.delete_job(&payload.job_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn run_job_now(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.run_job_now(&payload.job_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn enable_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.enable_job(&payload.job_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn disable_job(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<JobIdPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.disable_job(&payload.job_id).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_datastreams(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<DatastreamsPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.get_datastreams(payload.force).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_datastream_detail(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<DatastreamPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state
        .daemon
        .get_datastream_detail(&payload.datastream_id)
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn get_csv_preview(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<CsvPreviewPayload>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Some(response) = authorize(&headers, &state.token) {
        return response;
    }
    let payload = match parse_json_payload(payload) {
        Ok(payload) => payload,
        Err(error) => return command_error(error),
    };

    match state.daemon.get_csv_preview(payload.path, payload.rows) {
        Ok(response) => Json(response).into_response(),
        Err(error) => command_error(error),
    }
}

async fn status_stream(
    State(state): State<ApiState>,
    Query(query): Query<AccessTokenQuery>,
) -> Response {
    if query.access_token != state.token {
        return (StatusCode::UNAUTHORIZED, "Invalid access token.").into_response();
    }

    let stream = WatchStream::new(state.daemon.subscribe_status()).map(|snapshot| {
        let payload = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
        Ok::<Event, Infallible>(Event::default().event("status").data(payload))
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
        .into_response()
}
