use super::*;
use chrono::TimeZone;

#[test]
fn observation_batch_payload_matches_sensorthings_schema() {
    let observations = [
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

fn session_for(cache: &Arc<Mutex<TokenCache>>, url: &str, username: &str) -> HydroServerSession {
    HydroServerSession::new(Client::new(), userpass_server(url, username), cache.clone())
}

#[test]
fn token_cache_key_ignores_trailing_slash_and_surrounding_whitespace() {
    let cache = Arc::new(Mutex::new(TokenCache::new()));
    let a = session_for(&cache, "https://hydro.example/", "  user@example.com ");
    let b = session_for(&cache, "https://hydro.example", "user@example.com");
    assert_eq!(a.token_cache_key(), b.token_cache_key());
}

#[test]
fn cached_token_is_reused_across_sessions_for_the_same_account() {
    let cache = Arc::new(Mutex::new(TokenCache::new()));

    session_for(&cache, "https://hydro.example", "user@example.com").store_token("tok-123");

    let next = session_for(&cache, "https://hydro.example", "user@example.com");
    assert_eq!(next.cached_token().as_deref(), Some("tok-123"));
}

#[test]
fn cached_token_is_scoped_per_account() {
    let cache = Arc::new(Mutex::new(TokenCache::new()));
    session_for(&cache, "https://hydro.example", "alice@example.com").store_token("alice-tok");

    let bob = session_for(&cache, "https://hydro.example", "bob@example.com");
    assert_eq!(bob.cached_token(), None);
}

#[test]
fn invalidate_token_clears_the_shared_cache_for_other_sessions() {
    let cache = Arc::new(Mutex::new(TokenCache::new()));

    let mut session = session_for(&cache, "https://hydro.example", "user@example.com");
    session.store_token("stale");
    assert_eq!(session.cached_token().as_deref(), Some("stale"));

    session.invalidate_token();

    assert_eq!(session.cached_token(), None, "own cache lookup is cleared");
    let other = session_for(&cache, "https://hydro.example", "user@example.com");
    assert_eq!(
        other.cached_token(),
        None,
        "the stale token is gone for future sessions too"
    );
}
