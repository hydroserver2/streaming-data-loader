use crate::models::ServiceStatusResponse;
use crate::service_paths::SERVICE_CONFIG_DIR_FLAG;

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use tauri::AppHandle;
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
    Error as WindowsServiceError,
};

pub(crate) const WINDOWS_SERVICE_NAME: &str = "StreamingDataLoader";
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "Streaming Data Loader";
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "Background CSV watcher and uploader for Streaming Data Loader.";
const WINDOWS_SERVICE_ACTION_FLAG: &str = "--windows-service-action";
const WINDOWS_SERVICE_RESULT_FLAG: &str = "--windows-service-result-file";
const WINDOWS_SERVICE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const WINDOWS_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(500);
const WINDOWS_DAEMON_PID_FILENAME: &str = "daemon.pid";
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERROR_SERVICE_ALREADY_RUNNING: i32 = 1056;
const ERROR_SERVICE_NOT_ACTIVE: i32 = 1062;
const ERROR_SERVICE_EXISTS: i32 = 1073;
const ERROR_SERVICE_MARKED_FOR_DELETE: i32 = 1072;

pub fn get_service_status() -> Result<ServiceStatusResponse, String> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::ENUMERATE_SERVICE,
    )
    .map_err(format_windows_service_error)?;

    let service = match manager.open_service(
        WINDOWS_SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
    ) {
        Ok(service) => service,
        Err(error) if windows_service_unavailable(&error) => {
            return Ok(ServiceStatusResponse {
                supported: true,
                installed: false,
                running: false,
                label: WINDOWS_SERVICE_DISPLAY_NAME.to_string(),
                plist_path: String::new(),
                executable_path: String::new(),
                status_message: String::new(),
            });
        }
        Err(error) => return Err(format_windows_service_error(error)),
    };

    let status = service
        .query_status()
        .map_err(format_windows_service_error)?;
    let config = service
        .query_config()
        .map_err(format_windows_service_error)?;
    let running = windows_service_is_active(status.current_state);

    let status_message = match running {
        true => {
            "The background service is installed and running. It will persist app closure and user logout."
                .to_string()
        }
        false => {
            "The background service is installed but not currently running. Restart it to resume background loading."
                .to_string()
        }
    };

    Ok(ServiceStatusResponse {
        supported: true,
        installed: true,
        running,
        label: WINDOWS_SERVICE_DISPLAY_NAME.to_string(),
        plist_path: String::new(),
        executable_path: config.executable_path.to_string_lossy().into_owned(),
        status_message,
    })
}

#[cfg(windows)]
fn install_windows_service(config_dir: Option<PathBuf>) -> Result<(), String> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(format_windows_service_error)?;

    if let Ok(_existing) = manager.open_service(WINDOWS_SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        return Err("The background service is already installed.".to_string());
    }

    let config_dir =
        config_dir.unwrap_or(crate::service_paths::resolve_shared_service_config_dir()?);

    stop_existing_windows_daemon(&config_dir)?;

    let executable_path = std::env::current_exe().map_err(|err| err.to_string())?;
    tracing::info!(
        executable_path = %executable_path.display(),
        config_dir = %config_dir.display(),
        "installing Windows background service"
    );
    let service_info = ServiceInfo {
        name: OsString::from(WINDOWS_SERVICE_NAME),
        display_name: OsString::from(WINDOWS_SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path,
        launch_arguments: vec![
            OsString::from("--service"),
            windows_service_config_dir_launch_argument(&config_dir),
        ],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = manager
        .create_service(
            &service_info,
            ServiceAccess::QUERY_STATUS
                | ServiceAccess::START
                | ServiceAccess::STOP
                | ServiceAccess::DELETE
                | ServiceAccess::CHANGE_CONFIG,
        )
        .map_err(format_windows_service_error)?;

    let _ = service.set_description(WINDOWS_SERVICE_DESCRIPTION);
    let empty_args: [&OsStr; 0] = [];
    service
        .start(&empty_args)
        .map_err(format_windows_service_error)?;
    wait_for_windows_service_state(&service, ServiceState::Running)
}

pub fn install_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_windows_elevated_action(app_handle, "install")?;
    get_service_status()
}

pub fn restart_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_windows_elevated_action(app_handle, "restart")?;
    get_service_status()
}

pub fn uninstall_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_windows_elevated_action(app_handle, "uninstall")?;
    get_service_status()
}

pub fn maybe_handle_service_management_cli() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let mut action: Option<OsString> = None;
    let mut result_file: Option<PathBuf> = None;
    let mut config_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new(WINDOWS_SERVICE_ACTION_FLAG) {
            action = args.next();
        } else if let Some(value) =
            inline_windows_path_flag_value(&arg, WINDOWS_SERVICE_RESULT_FLAG)
        {
            result_file = Some(value);
        } else if arg == OsStr::new(WINDOWS_SERVICE_RESULT_FLAG) {
            result_file = args.next().map(PathBuf::from);
        } else if let Some(value) = inline_windows_path_flag_value(&arg, SERVICE_CONFIG_DIR_FLAG) {
            config_dir = Some(value);
        } else if arg == OsStr::new(SERVICE_CONFIG_DIR_FLAG) {
            config_dir = args.next().map(PathBuf::from);
        }
    }

    let action = action?;
    let result = run_windows_management_action(action.as_os_str(), config_dir);

    if let Some(path) = result_file {
        match &result {
            Ok(()) => {
                let _ = fs::write(&path, "");
            }
            Err(message) => {
                let _ = fs::write(&path, message);
            }
        }
    }

    Some(if result.is_ok() { 0 } else { 1 })
}

fn run_windows_elevated_action(app_handle: &AppHandle, action: &str) -> Result<(), String> {
    let executable_path = service_executable_path()?;
    let result_path = temp_result_path("windows-service");
    let config_dir = crate::runtime::resolve_config_dir(app_handle)?;
    tracing::info!(
        action,
        executable_path = %executable_path.display(),
        config_dir = %config_dir.display(),
        "requesting elevated Windows background service action"
    );
    let result_file_arg =
        windows_inline_path_flag_argument(WINDOWS_SERVICE_RESULT_FLAG, &result_path);
    let config_dir_arg = windows_inline_path_flag_argument(SERVICE_CONFIG_DIR_FLAG, &config_dir);
    let script = format!(
        "$proc = Start-Process -FilePath '{}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('{}', '{}', '{}', '{}'); exit $proc.ExitCode",
        powershell_quote(&executable_path.to_string_lossy()),
        WINDOWS_SERVICE_ACTION_FLAG,
        action,
        powershell_quote(&result_file_arg),
        powershell_quote(&config_dir_arg)
    );

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|err| format!("Couldn't launch the Windows elevation prompt: {err}"))?;

    let message = fs::read_to_string(&result_path)
        .ok()
        .map(|contents| contents.trim().to_string())
        .filter(|contents| !contents.is_empty());
    let _ = fs::remove_file(&result_path);

    if status.success() {
        tracing::info!(action, "Windows background service action completed");
        return Ok(());
    }

    let message = message.unwrap_or_else(|| {
        "The Windows background service action failed or was canceled.".to_string()
    });
    tracing::error!(action, error = %message, "Windows background service action failed");
    Err(message)
}

fn service_executable_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|err| err.to_string())
}

fn temp_result_path(kind: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    std::env::temp_dir().join(format!(
        "sdl-{kind}-{}.txt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ))
}

fn run_windows_management_action(
    action: &OsStr,
    config_dir: Option<PathBuf>,
) -> Result<(), String> {
    tracing::info!(
        action = %action.to_string_lossy(),
        config_dir = %config_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<default>".to_string()),
        "handling Windows background service management action"
    );
    match action.to_string_lossy().as_ref() {
        "install" => install_windows_service(config_dir),
        "restart" => restart_windows_service(config_dir),
        "uninstall" => uninstall_windows_service(config_dir),
        _ => Err("Unknown Windows service action.".to_string()),
    }
}

fn restart_windows_service(config_dir: Option<PathBuf>) -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(format_windows_service_error)?;
    let service_access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::QUERY_CONFIG
        | ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::START
        | ServiceAccess::STOP;
    let service = manager
        .open_service(WINDOWS_SERVICE_NAME, service_access)
        .map_err(format_windows_service_error)?;

    if let Some(config_dir) = config_dir {
        tracing::info!(config_dir = %config_dir.display(), "syncing Windows service launch config before restart");
        sync_windows_service_launch_config(&service, &config_dir)?;
    }

    tracing::info!("restarting Windows background service");
    stop_windows_service_if_needed(&service)?;
    let empty_args: [&OsStr; 0] = [];
    service
        .start(&empty_args)
        .map_err(format_windows_service_error)?;
    wait_for_windows_service_state(&service, ServiceState::Running)
}

fn uninstall_windows_service(_config_dir: Option<PathBuf>) -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(format_windows_service_error)?;
    let service = manager
        .open_service(
            WINDOWS_SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(format_windows_service_error)?;

    tracing::info!("uninstalling Windows background service");
    stop_windows_service_if_needed(&service)?;
    service.delete().map_err(format_windows_service_error)
}

fn stop_existing_windows_daemon(config_dir: &Path) -> Result<(), String> {
    let pid_path = config_dir.join(WINDOWS_DAEMON_PID_FILENAME);
    let endpoint_path = crate::service_paths::daemon_endpoint_path(config_dir);

    let Some(pid) = fs::read_to_string(&pid_path)
        .ok()
        .and_then(|contents| contents.trim().parse::<u32>().ok())
    else {
        return Ok(());
    };

    let stop_script = format!(
        "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ Stop-Process -Id {pid} -Force -ErrorAction SilentlyContinue }}"
    );
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &stop_script,
        ])
        .status();

    std::thread::sleep(Duration::from_secs(1));

    let _ = fs::remove_file(&endpoint_path);
    let _ = fs::remove_file(&pid_path);
    Ok(())
}

fn sync_windows_service_launch_config(
    service: &windows_service::service::Service,
    config_dir: &Path,
) -> Result<(), String> {
    let existing_config = service
        .query_config()
        .map_err(format_windows_service_error)?;
    let desired_launch_arguments = vec![
        OsString::from("--service"),
        windows_service_config_dir_launch_argument(config_dir),
    ];

    let desired_config = ServiceInfo {
        name: OsString::from(WINDOWS_SERVICE_NAME),
        display_name: existing_config.display_name,
        service_type: existing_config.service_type,
        start_type: existing_config.start_type,
        error_control: existing_config.error_control,
        executable_path: existing_config.executable_path,
        launch_arguments: desired_launch_arguments,
        dependencies: existing_config.dependencies,
        account_name: existing_config.account_name,
        account_password: None,
    };

    service
        .change_config(&desired_config)
        .map_err(format_windows_service_error)
}

fn stop_windows_service_if_needed(
    service: &windows_service::service::Service,
) -> Result<(), String> {
    let status = service
        .query_status()
        .map_err(format_windows_service_error)?;
    match status.current_state {
        ServiceState::Stopped => Ok(()),
        ServiceState::StopPending => wait_for_windows_service_state(service, ServiceState::Stopped),
        _ => {
            service.stop().map_err(format_windows_service_error)?;
            wait_for_windows_service_state(service, ServiceState::Stopped)
        }
    }
}

fn wait_for_windows_service_state(
    service: &windows_service::service::Service,
    desired_state: ServiceState,
) -> Result<(), String> {
    let deadline = Instant::now() + WINDOWS_SERVICE_WAIT_TIMEOUT;

    loop {
        let status = service
            .query_status()
            .map_err(format_windows_service_error)?;
        if status.current_state == desired_state {
            return Ok(());
        }

        if Instant::now() >= deadline {
            return Err(match desired_state {
                ServiceState::Running => {
                    "Timed out waiting for the Windows background service to start.".to_string()
                }
                ServiceState::Stopped => {
                    "Timed out waiting for the Windows background service to stop.".to_string()
                }
                _ => "Timed out waiting for the Windows background service.".to_string(),
            });
        }

        std::thread::sleep(WINDOWS_STATUS_POLL_INTERVAL);
    }
}

fn windows_service_is_active(state: ServiceState) -> bool {
    matches!(
        state,
        ServiceState::Running | ServiceState::StartPending | ServiceState::ContinuePending
    )
}

fn windows_service_not_found(error: &WindowsServiceError) -> bool {
    match error {
        WindowsServiceError::Winapi(io_error) => {
            io_error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST)
        }
        _ => false,
    }
}

fn windows_service_unavailable(error: &WindowsServiceError) -> bool {
    windows_service_not_found(error) || windows_service_marked_for_delete(error)
}

fn windows_service_marked_for_delete(error: &WindowsServiceError) -> bool {
    match error {
        WindowsServiceError::Winapi(io_error) => {
            io_error.raw_os_error() == Some(ERROR_SERVICE_MARKED_FOR_DELETE)
        }
        _ => false,
    }
}

fn format_windows_service_error(error: WindowsServiceError) -> String {
    match error {
        WindowsServiceError::Winapi(io_error) => match io_error.raw_os_error() {
            Some(ERROR_SERVICE_DOES_NOT_EXIST) => {
                "The background service is not installed.".to_string()
            }
            Some(ERROR_SERVICE_MARKED_FOR_DELETE) => {
                "The background service is being removed.".to_string()
            }
            Some(ERROR_SERVICE_ALREADY_RUNNING) => {
                "The background service is already running.".to_string()
            }
            Some(ERROR_SERVICE_NOT_ACTIVE) => "The background service is not running.".to_string(),
            Some(ERROR_SERVICE_EXISTS) => {
                "The background service is already installed.".to_string()
            }
            Some(5) => "Administrator privileges are required to manage the background service."
                .to_string(),
            _ => io_error.to_string(),
        },
        _ => error.to_string(),
    }
}

fn powershell_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn windows_service_config_dir_launch_argument(config_dir: &Path) -> OsString {
    OsString::from(format!(
        r#"{SERVICE_CONFIG_DIR_FLAG}="{}""#,
        config_dir.to_string_lossy()
    ))
}

fn windows_inline_path_flag_argument(flag: &str, path: &Path) -> String {
    format!(r#"{flag}="{}""#, path.to_string_lossy())
}

fn inline_windows_path_flag_value(arg: &OsString, flag: &str) -> Option<PathBuf> {
    let prefix = format!("{flag}=");
    let value = arg.to_string_lossy();
    let raw_path = value.strip_prefix(&prefix)?;
    let path = raw_path.trim_matches('"');
    if path.is_empty() {
        return None;
    }

    Some(PathBuf::from(path))
}
