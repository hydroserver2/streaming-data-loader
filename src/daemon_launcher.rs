use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use tauri::AppHandle;

use crate::{
    daemon_api::read_connection_info, models::DaemonConnectionInfo, runtime,
    service_paths::resolve_shared_service_config_dir,
};

const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(8);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(150);

pub async fn ensure_daemon_connection(
    app_handle: &AppHandle,
) -> Result<DaemonConnectionInfo, String> {
    let _ = runtime::resolve_config_dir(app_handle)?;
    let config_dir = resolve_shared_service_config_dir()?;

    if let Some(connection) = read_live_connection(config_dir.clone()).await? {
        return Ok(connection);
    }

    spawn_daemon_process(resolve_service_executable_path()?)?;

    let started_at = std::time::Instant::now();
    loop {
        if let Some(connection) = read_live_connection(config_dir.clone()).await? {
            return Ok(connection);
        }

        if started_at.elapsed() >= DAEMON_STARTUP_TIMEOUT {
            return Err("The daemon did not become ready in time.".to_string());
        }

        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
    }
}

async fn read_live_connection(config_dir: PathBuf) -> Result<Option<DaemonConnectionInfo>, String> {
    let connection = match read_connection_info(config_dir) {
        Ok(connection) => connection,
        Err(error) if is_missing_endpoint_error(&error) => return Ok(None),
        Err(error) => return Err(error),
    };

    if ping(&connection).await {
        return Ok(Some(connection));
    }

    Ok(None)
}

fn is_missing_endpoint_error(error: &str) -> bool {
    error.contains("No such file") || error.contains("cannot find the file")
}

async fn ping(connection: &DaemonConnectionInfo) -> bool {
    let client = reqwest::Client::new();
    let Ok(response) = client
        .post(format!(
            "{}/api/commands/ping",
            connection.base_url.trim_end_matches('/')
        ))
        .bearer_auth(&connection.token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
    else {
        return false;
    };

    response.status().is_success()
}

fn spawn_daemon_process(executable_path: PathBuf) -> Result<(), String> {
    Command::new(executable_path)
        .arg("--service")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn resolve_service_executable_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage_path) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage_path));
    }

    std::env::current_exe().map_err(|err| err.to_string())
}
