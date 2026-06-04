use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use tauri::AppHandle;

use crate::{
    daemon_api::{read_connection_info, ConnectionReadError},
    models::DaemonConnectionInfo,
    runtime,
    service_paths::resolve_shared_service_config_dir,
};

const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(150);

pub async fn ensure_daemon_connection(
    app_handle: &AppHandle,
) -> Result<DaemonConnectionInfo, String> {
    let _ = runtime::resolve_config_dir(app_handle)?;
    let config_dir = resolve_shared_service_config_dir()?;

    if let Some(connection) = read_live_connection(config_dir.clone()).await? {
        return Ok(connection);
    }

    let service_status = crate::service::get_service_status()?;
    match connect_strategy(
        service_status.supported,
        service_status.installed,
        service_status.running,
        cfg!(windows),
    ) {
        // The service owns the daemon but hasn't published a reachable endpoint
        // yet (it may still be starting up after login or a reboot). Wait for it.
        ConnectStrategy::AwaitService => wait_for_live_connection(config_dir).await,
        ConnectStrategy::ServiceStopped => {
            Err("Restart the background service to continue.".to_string())
        }
        ConnectStrategy::ServiceRequired => {
            Err("Install the background service to continue.".to_string())
        }
        ConnectStrategy::SpawnAdHoc => {
            spawn_daemon_process(resolve_service_executable_path()?)?;
            wait_for_live_connection(config_dir).await
        }
    }
}

/// How to obtain a daemon connection once no live one already exists.
#[derive(Debug, PartialEq, Eq)]
enum ConnectStrategy {
    /// A managed service owns the daemon and is running; wait for its endpoint.
    AwaitService,
    /// A managed service is installed but stopped; ask the user to restart it.
    ServiceStopped,
    /// The platform mandates the managed service, but it isn't installed.
    ServiceRequired,
    /// No managed service is in play; run a daemon ad-hoc for this session.
    SpawnAdHoc,
}

/// Decide how to connect given the managed-service status.
///
/// When a managed service is installed it is the sole owner of the daemon: the
/// macOS LaunchDaemon and the Windows service each run it under their own
/// account and enforce a single instance via a pid lock. Spawning our own daemon
/// alongside it would race for that lock and the endpoint file, leaving a
/// user-owned process and the service fighting — and on macOS can wedge the
/// service into a restart loop. So whenever the service is installed we defer to
/// it rather than spawning a competitor.
///
/// `windows_requires_service` captures the platform rule that Windows has no
/// ad-hoc daemon mode, so a missing service there is a hard stop; macOS and Linux
/// fall back to running a daemon ad-hoc when no service is installed.
fn connect_strategy(
    supported: bool,
    installed: bool,
    running: bool,
    windows_requires_service: bool,
) -> ConnectStrategy {
    if supported && installed {
        return if running {
            ConnectStrategy::AwaitService
        } else {
            ConnectStrategy::ServiceStopped
        };
    }

    if supported && windows_requires_service {
        return ConnectStrategy::ServiceRequired;
    }

    ConnectStrategy::SpawnAdHoc
}

async fn wait_for_live_connection(config_dir: PathBuf) -> Result<DaemonConnectionInfo, String> {
    let started_at = Instant::now();
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
        Err(ConnectionReadError::MissingEndpoint) | Err(ConnectionReadError::Incomplete) => {
            return Ok(None);
        }
        Err(ConnectionReadError::Fatal(error)) => return Err(error),
    };

    if ping(&connection).await {
        return Ok(Some(connection));
    }

    Ok(None)
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

#[cfg(test)]
#[path = "tests/daemon_launcher.rs"]
mod tests;
