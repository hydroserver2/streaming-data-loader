use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::AppHandle;

use crate::{
    models::ServiceStatusResponse,
    service_paths::{active_app_directory_name, default_shared_service_config_dir},
};

#[cfg(windows)]
use std::{
    ffi::{OsStr, OsString},
    time::{Duration, Instant},
};

#[cfg(windows)]
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceStatus, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
    Error as WindowsServiceError,
};

#[cfg(target_os = "linux")]
use std::ffi::{OsStr, OsString};

#[cfg(target_os = "macos")]
const SERVICE_LABEL: &str = "com.hydroserver.sdl";
#[cfg(target_os = "macos")]
const SERVICE_PLIST_PATH: &str = "/Library/LaunchDaemons/com.hydroserver.sdl.plist";

#[cfg(target_os = "linux")]
const LINUX_SERVICE_NAME: &str = "streaming-data-loader.service";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_DISPLAY_NAME: &str = "Streaming Data Loader";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_PATH: &str = "/etc/systemd/system/streaming-data-loader.service";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_ACTION_FLAG: &str = "--linux-service-action";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_RESULT_FLAG: &str = "--linux-service-result-file";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_USER_FLAG: &str = "--linux-service-user";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_HOME_FLAG: &str = "--linux-service-home";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_CONFIG_FLAG: &str = "--linux-service-config-dir";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_EXEC_FLAG: &str = "--linux-service-exec-path";

#[cfg(windows)]
pub(crate) const WINDOWS_SERVICE_NAME: &str = "StreamingDataLoader";
#[cfg(windows)]
const WINDOWS_SERVICE_DISPLAY_NAME: &str = "Streaming Data Loader";
#[cfg(windows)]
const WINDOWS_SERVICE_DESCRIPTION: &str =
    "Background CSV watcher and uploader for Streaming Data Loader.";
#[cfg(windows)]
const WINDOWS_SERVICE_ACTION_FLAG: &str = "--windows-service-action";
#[cfg(windows)]
const WINDOWS_SERVICE_RESULT_FLAG: &str = "--windows-service-result-file";
#[cfg(windows)]
const WINDOWS_SERVICE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const WINDOWS_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(windows)]
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
#[cfg(windows)]
const ERROR_SERVICE_ALREADY_RUNNING: i32 = 1056;
#[cfg(windows)]
const ERROR_SERVICE_NOT_ACTIVE: i32 = 1062;
#[cfg(windows)]
const ERROR_SERVICE_EXISTS: i32 = 1073;

pub fn get_service_status(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    #[cfg(target_os = "macos")]
    {
        return get_macos_service_status(app_handle);
    }

    #[cfg(target_os = "linux")]
    {
        let _ = app_handle;
        return get_linux_service_status();
    }

    #[cfg(windows)]
    {
        let _ = app_handle;
        return get_windows_service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = app_handle;
        Ok(unsupported_service_status(
            "Background service management is only available on macOS, Windows, and Linux systemd hosts.",
        ))
    }
}

pub fn install_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    #[cfg(target_os = "macos")]
    {
        return install_macos_service(app_handle);
    }

    #[cfg(target_os = "linux")]
    {
        run_linux_elevated_action(app_handle, "install")?;
        return get_linux_service_status();
    }

    #[cfg(windows)]
    {
        run_windows_elevated_action(app_handle, "install")?;
        return get_windows_service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = app_handle;
        Err("Background service management isn't supported on this OS.".to_string())
    }
}

pub fn restart_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    #[cfg(target_os = "macos")]
    {
        return restart_macos_service(app_handle);
    }

    #[cfg(target_os = "linux")]
    {
        run_linux_elevated_action(app_handle, "restart")?;
        return get_linux_service_status();
    }

    #[cfg(windows)]
    {
        run_windows_elevated_action(app_handle, "restart")?;
        return get_windows_service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = app_handle;
        Err("Background service management isn't supported on this OS.".to_string())
    }
}

pub fn uninstall_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    #[cfg(target_os = "macos")]
    {
        return uninstall_macos_service(app_handle);
    }

    #[cfg(target_os = "linux")]
    {
        run_linux_elevated_action(app_handle, "uninstall")?;
        return get_linux_service_status();
    }

    #[cfg(windows)]
    {
        run_windows_elevated_action(app_handle, "uninstall")?;
        return get_windows_service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = app_handle;
        Err("Background service management isn't supported on this OS.".to_string())
    }
}

pub fn maybe_handle_service_management_cli() -> Option<i32> {
    #[cfg(target_os = "linux")]
    {
        return maybe_handle_linux_management_cli();
    }

    #[cfg(windows)]
    {
        return maybe_handle_windows_management_cli();
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

#[allow(dead_code)]
fn unsupported_service_status(message: &str) -> ServiceStatusResponse {
    ServiceStatusResponse {
        supported: false,
        installed: false,
        running: false,
        label: String::new(),
        plist_path: String::new(),
        executable_path: String::new(),
        status_message: message.to_string(),
    }
}

#[cfg(target_os = "macos")]
fn get_macos_service_status(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    let executable_path = service_executable_path(app_handle)?;
    let plist_path = PathBuf::from(SERVICE_PLIST_PATH);
    let installed = plist_path.exists();
    let launchctl_output = launchctl_print_output();
    let launchctl_running = launchctl_output
        .as_deref()
        .map(|output| output.contains("state = running") || output.contains("pid ="))
        .unwrap_or(false);
    let process_running = daemon_process_running();
    let running = installed && (launchctl_running || process_running);

    let status_message = match (installed, running) {
        (false, _) => String::new(),
        (true, true) => {
            "The background service is installed and running. It will persist app closure and user logout."
                .to_string()
        }
        (true, false) => {
            "The background service is installed but not currently running. Restart it to resume background loading."
                .to_string()
        }
    };

    Ok(ServiceStatusResponse {
        supported: true,
        installed,
        running,
        label: SERVICE_LABEL.to_string(),
        plist_path: plist_path.to_string_lossy().into_owned(),
        executable_path: executable_path.to_string_lossy().into_owned(),
        status_message,
    })
}

#[cfg(target_os = "macos")]
fn install_macos_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    let plist_contents = render_macos_plist(app_handle)?;
    let temp_path = write_temp_script(
        "install",
        &format!(
            "set -e\nmkdir -p {shared_dir} {logs_dir}\ncat > {temp_plist} <<'PLIST'\n{plist}\nPLIST\ncp {temp_plist} {system_plist}\nchmod 644 {system_plist}\nchown root:wheel {system_plist}\n/bin/launchctl bootout system {system_plist} >/dev/null 2>&1 || true\n/bin/launchctl bootstrap system {system_plist}\n/bin/launchctl kickstart -k system/{label} >/dev/null 2>&1 || true\nrm -f {temp_plist}\n",
            shared_dir = shell_quote(default_shared_service_config_dir()?.to_string_lossy().as_ref()),
            logs_dir = shell_quote(
                default_shared_service_config_dir()?
                    .join("logs")
                    .to_string_lossy()
                    .as_ref()
            ),
            temp_plist = shell_quote(temp_plist_path().to_string_lossy().as_ref()),
            plist = plist_contents,
            system_plist = shell_quote(SERVICE_PLIST_PATH),
            label = SERVICE_LABEL,
        ),
    )?;

    let result = run_macos_elevated_script(&temp_path);
    let _ = fs::remove_file(&temp_path);
    result?;
    get_macos_service_status(app_handle)
}

#[cfg(target_os = "macos")]
fn restart_macos_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    if !Path::new(SERVICE_PLIST_PATH).exists() {
        return Err("The background service is not installed.".to_string());
    }

    let temp_path = write_temp_script(
        "restart",
        &format!(
            "set -e\n/bin/launchctl bootout system {system_plist} >/dev/null 2>&1 || true\n/bin/launchctl bootstrap system {system_plist}\n/bin/launchctl kickstart -k system/{label}\n",
            system_plist = shell_quote(SERVICE_PLIST_PATH),
            label = SERVICE_LABEL,
        ),
    )?;

    let result = run_macos_elevated_script(&temp_path);
    let _ = fs::remove_file(&temp_path);
    result?;
    get_macos_service_status(app_handle)
}

#[cfg(target_os = "macos")]
fn uninstall_macos_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    let temp_path = write_temp_script(
        "uninstall",
        &format!(
            "set -e\n/bin/launchctl bootout system {system_plist} >/dev/null 2>&1 || true\nrm -f {system_plist}\n",
            system_plist = shell_quote(SERVICE_PLIST_PATH),
        ),
    )?;

    let result = run_macos_elevated_script(&temp_path);
    let _ = fs::remove_file(&temp_path);
    result?;
    get_macos_service_status(app_handle)
}

#[cfg(target_os = "macos")]
fn render_macos_plist(app_handle: &AppHandle) -> Result<String, String> {
    let executable_path = service_executable_path(app_handle)?;
    let shared_dir = default_shared_service_config_dir()?;
    let logs_dir = shared_dir.join("logs");

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>

  <key>ProgramArguments</key>
  <array>
    <string>{program}</string>
    <string>--service</string>
  </array>

  <key>RunAtLoad</key>
  <true/>

  <key>KeepAlive</key>
  <true/>

  <key>WorkingDirectory</key>
  <string>{working_dir}</string>

  <key>StandardOutPath</key>
  <string>{stdout_path}</string>

  <key>StandardErrorPath</key>
  <string>{stderr_path}</string>
</dict>
</plist>
"#,
        label = SERVICE_LABEL,
        program = xml_escape(&executable_path.to_string_lossy()),
        working_dir = xml_escape(&shared_dir.to_string_lossy()),
        stdout_path = xml_escape(&logs_dir.join("daemon.stdout.log").to_string_lossy()),
        stderr_path = xml_escape(&logs_dir.join("daemon.stderr.log").to_string_lossy()),
    ))
}

#[cfg(target_os = "macos")]
fn launchctl_print_output() -> Option<String> {
    let output = Command::new("/bin/launchctl")
        .arg("print")
        .arg(format!("system/{SERVICE_LABEL}"))
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "macos")]
fn daemon_process_running() -> bool {
    let Ok(output) = Command::new("/usr/bin/pgrep")
        .arg("-af")
        .arg("streaming-data-loader --service")
        .output()
    else {
        return false;
    };

    output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

#[cfg(target_os = "macos")]
fn run_macos_elevated_script(script_path: &Path) -> Result<(), String> {
    let command = format!("/bin/sh {}", script_path.display());
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(format!(
            r#"do shell script "{}" with administrator privileges"#,
            applescript_escape(&command)
        ))
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = stderr
        .strip_prefix("execution error: ")
        .unwrap_or(&stderr)
        .trim()
        .to_string();

    if !message.is_empty() {
        return Err(message);
    }
    if !stdout.is_empty() {
        return Err(stdout);
    }

    Err("The background service command did not complete.".to_string())
}

#[cfg(target_os = "linux")]
fn get_linux_service_status() -> Result<ServiceStatusResponse, String> {
    if !linux_systemd_supported()? {
        return Ok(unsupported_service_status(
            "Background service management requires a Linux systemd host with systemctl available.",
        ));
    }

    let properties = linux_query_service_properties()?;
    let load_state = properties
        .get("LoadState")
        .map(String::as_str)
        .unwrap_or("not-found");
    let active_state = properties
        .get("ActiveState")
        .map(String::as_str)
        .unwrap_or("inactive");
    let unit_file_state = properties
        .get("UnitFileState")
        .map(String::as_str)
        .unwrap_or("bad");
    let fragment_path = properties
        .get("FragmentPath")
        .cloned()
        .unwrap_or_else(|| LINUX_SERVICE_PATH.to_string());

    let installed = Path::new(LINUX_SERVICE_PATH).exists()
        || load_state != "not-found"
        || matches!(
            unit_file_state,
            "enabled"
                | "enabled-runtime"
                | "disabled"
                | "static"
                | "indirect"
                | "linked"
                | "linked-runtime"
                | "alias"
                | "masked"
        );
    let running = matches!(active_state, "active" | "activating" | "reloading");

    let status_message = match (installed, running) {
        (false, _) => String::new(),
        (true, true) => {
            "The background service is installed and running. It will persist app closure and user logout."
                .to_string()
        }
        (true, false) => {
            "The background service is installed but not currently running. Restart it to resume background loading."
                .to_string()
        }
    };

    Ok(ServiceStatusResponse {
        supported: true,
        installed,
        running,
        label: LINUX_SERVICE_DISPLAY_NAME.to_string(),
        plist_path: fragment_path,
        executable_path: String::new(),
        status_message,
    })
}

#[cfg(target_os = "linux")]
fn linux_systemd_supported() -> Result<bool, String> {
    let output = match Command::new("systemctl")
        .args(["show", "--property=Version", "--value"])
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.to_string()),
    };

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("System has not been booted with systemd") {
        return Ok(false);
    }

    Ok(false)
}

#[cfg(target_os = "linux")]
fn linux_query_service_properties() -> Result<std::collections::HashMap<String, String>, String> {
    let output = Command::new("systemctl")
        .args([
            "show",
            LINUX_SERVICE_NAME,
            "--property=LoadState,ActiveState,UnitFileState,FragmentPath",
            "--no-page",
        ])
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("System has not been booted with systemd") {
            return Err(
                "Background service management requires systemd and is unavailable on this Linux host."
                    .to_string(),
            );
        }
        if !stderr.is_empty() {
            return Err(stderr);
        }
    }

    Ok(parse_systemctl_properties(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(target_os = "linux")]
fn maybe_handle_linux_management_cli() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let mut action: Option<OsString> = None;
    let mut result_file: Option<PathBuf> = None;
    let mut user: Option<OsString> = None;
    let mut home: Option<OsString> = None;
    let mut config_dir: Option<PathBuf> = None;
    let mut exec_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new(LINUX_SERVICE_ACTION_FLAG) {
            action = args.next();
        } else if arg == OsStr::new(LINUX_SERVICE_RESULT_FLAG) {
            result_file = args.next().map(PathBuf::from);
        } else if arg == OsStr::new(LINUX_SERVICE_USER_FLAG) {
            user = args.next();
        } else if arg == OsStr::new(LINUX_SERVICE_HOME_FLAG) {
            home = args.next();
        } else if arg == OsStr::new(LINUX_SERVICE_CONFIG_FLAG) {
            config_dir = args.next().map(PathBuf::from);
        } else if arg == OsStr::new(LINUX_SERVICE_EXEC_FLAG) {
            exec_path = args.next().map(PathBuf::from);
        }
    }

    let action = action?;
    let result = run_linux_management_action(
        action.as_os_str(),
        LinuxServiceContext {
            user,
            home,
            config_dir,
            exec_path,
        },
    );

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

#[cfg(target_os = "linux")]
struct LinuxServiceContext {
    user: Option<OsString>,
    home: Option<OsString>,
    config_dir: Option<PathBuf>,
    exec_path: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
fn run_linux_elevated_action(app_handle: &AppHandle, action: &str) -> Result<(), String> {
    let executable_path = service_executable_path(app_handle)?;
    let result_path = temp_result_path("linux-service");
    let config_dir = crate::runtime::resolve_config_dir(app_handle)?;
    let user = std::env::var_os("USER")
        .or_else(|| std::env::var_os("LOGNAME"))
        .ok_or_else(|| {
            "Couldn't determine the Linux account for the background service.".to_string()
        })?;
    let home = std::env::var_os("HOME")
        .ok_or_else(|| "Couldn't determine the current user's home directory.".to_string())?;

    let status = Command::new("pkexec")
        .arg(&executable_path)
        .arg(LINUX_SERVICE_ACTION_FLAG)
        .arg(action)
        .arg(LINUX_SERVICE_RESULT_FLAG)
        .arg(&result_path)
        .arg(LINUX_SERVICE_USER_FLAG)
        .arg(&user)
        .arg(LINUX_SERVICE_HOME_FLAG)
        .arg(&home)
        .arg(LINUX_SERVICE_CONFIG_FLAG)
        .arg(&config_dir)
        .arg(LINUX_SERVICE_EXEC_FLAG)
        .arg(&executable_path)
        .status()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                "Couldn't find `pkexec`. Install polkit support to manage the Linux background service."
                    .to_string()
            } else {
                format!("Couldn't launch the Linux elevation prompt: {err}")
            }
        })?;

    let message = fs::read_to_string(&result_path)
        .ok()
        .map(|contents| contents.trim().to_string())
        .filter(|contents| !contents.is_empty());
    let _ = fs::remove_file(&result_path);

    if status.success() {
        return Ok(());
    }

    Err(message.unwrap_or_else(|| {
        "The Linux background service action failed or was canceled.".to_string()
    }))
}

#[cfg(target_os = "linux")]
fn run_linux_management_action(action: &OsStr, context: LinuxServiceContext) -> Result<(), String> {
    match action.to_string_lossy().as_ref() {
        "install" => install_linux_service(context),
        "restart" => restart_linux_service(),
        "uninstall" => uninstall_linux_service(),
        _ => Err("Unknown Linux service action.".to_string()),
    }
}

#[cfg(target_os = "linux")]
fn install_linux_service(context: LinuxServiceContext) -> Result<(), String> {
    let user = context
        .user
        .ok_or_else(|| "Missing Linux service user.".to_string())?;
    let home = context
        .home
        .ok_or_else(|| "Missing Linux service home directory.".to_string())?;
    let config_dir = context
        .config_dir
        .ok_or_else(|| "Missing Linux service config directory.".to_string())?;
    let exec_path = context
        .exec_path
        .ok_or_else(|| "Missing Linux service executable path.".to_string())?;

    fs::create_dir_all(&config_dir).map_err(|err| err.to_string())?;
    let unit_contents = render_linux_unit(
        user.to_string_lossy().as_ref(),
        home.to_string_lossy().as_ref(),
        &config_dir,
        &exec_path,
    );
    fs::write(LINUX_SERVICE_PATH, unit_contents).map_err(|err| err.to_string())?;

    run_systemctl(&["daemon-reload"])?;
    run_systemctl(&["enable", "--now", LINUX_SERVICE_NAME])?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn restart_linux_service() -> Result<(), String> {
    if !Path::new(LINUX_SERVICE_PATH).exists() {
        return Err("The background service is not installed.".to_string());
    }

    run_systemctl(&["restart", LINUX_SERVICE_NAME])
}

#[cfg(target_os = "linux")]
fn uninstall_linux_service() -> Result<(), String> {
    if Path::new(LINUX_SERVICE_PATH).exists() {
        let _ = run_systemctl(&["disable", "--now", LINUX_SERVICE_NAME]);
        let _ = run_systemctl(&["reset-failed", LINUX_SERVICE_NAME]);
        fs::remove_file(LINUX_SERVICE_PATH).map_err(|err| err.to_string())?;
        run_systemctl(&["daemon-reload"])?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn render_linux_unit(user: &str, home: &str, config_dir: &Path, exec_path: &Path) -> String {
    format!(
        "[Unit]\nDescription={display_name}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={user}\nWorkingDirectory={working_dir}\nEnvironment=\"HOME={home}\"\nEnvironment=\"SDL_CONFIG_DIR={config_dir}\"\nExecStart=\"{exec_path}\" --service\nRestart=always\nRestartSec=2\n\n[Install]\nWantedBy=multi-user.target\n",
        display_name = LINUX_SERVICE_DISPLAY_NAME,
        user = systemd_escape(user),
        working_dir = systemd_escape(&config_dir.to_string_lossy()),
        home = systemd_escape(home),
        config_dir = systemd_escape(&config_dir.to_string_lossy()),
        exec_path = systemd_escape(&exec_path.to_string_lossy()),
    )
}

#[cfg(target_os = "linux")]
fn run_systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        return Err(stderr);
    }
    if !stdout.is_empty() {
        return Err(stdout);
    }
    Err("The Linux system service command did not complete.".to_string())
}

#[cfg(target_os = "linux")]
fn parse_systemctl_properties(output: &str) -> std::collections::HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(windows)]
fn get_windows_service_status() -> Result<ServiceStatusResponse, String> {
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
        Err(error) if windows_service_not_found(&error) => {
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
fn maybe_handle_windows_management_cli() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let mut action: Option<OsString> = None;
    let mut result_file: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new(WINDOWS_SERVICE_ACTION_FLAG) {
            action = args.next();
        } else if arg == OsStr::new(WINDOWS_SERVICE_RESULT_FLAG) {
            result_file = args.next().map(PathBuf::from);
        }
    }

    let action = action?;
    let result = run_windows_management_action(action.as_os_str());

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

#[cfg(windows)]
fn run_windows_elevated_action(app_handle: &AppHandle, action: &str) -> Result<(), String> {
    let executable_path = service_executable_path(app_handle)?;
    let result_path = temp_result_path("windows-service");
    let script = format!(
        "$proc = Start-Process -FilePath '{}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('{}', '{}', '{}', '{}'); exit $proc.ExitCode",
        powershell_quote(&executable_path.to_string_lossy()),
        WINDOWS_SERVICE_ACTION_FLAG,
        action,
        WINDOWS_SERVICE_RESULT_FLAG,
        powershell_quote(&result_path.to_string_lossy())
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
        return Ok(());
    }

    Err(message.unwrap_or_else(|| {
        "The Windows background service action failed or was canceled.".to_string()
    }))
}

#[cfg(windows)]
fn run_windows_management_action(action: &OsStr) -> Result<(), String> {
    match action.to_string_lossy().as_ref() {
        "install" => install_windows_service(),
        "restart" => restart_windows_service(),
        "uninstall" => uninstall_windows_service(),
        _ => Err("Unknown Windows service action.".to_string()),
    }
}

#[cfg(windows)]
fn install_windows_service() -> Result<(), String> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(format_windows_service_error)?;

    if let Ok(_existing) = manager.open_service(WINDOWS_SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        return Err("The background service is already installed.".to_string());
    }

    let executable_path = std::env::current_exe().map_err(|err| err.to_string())?;
    let service_info = ServiceInfo {
        name: OsString::from(WINDOWS_SERVICE_NAME),
        display_name: OsString::from(WINDOWS_SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path,
        launch_arguments: vec![OsString::from("--service")],
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

#[cfg(windows)]
fn restart_windows_service() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(format_windows_service_error)?;
    let service = manager
        .open_service(
            WINDOWS_SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::STOP,
        )
        .map_err(format_windows_service_error)?;

    stop_windows_service_if_needed(&service)?;
    let empty_args: [&OsStr; 0] = [];
    service
        .start(&empty_args)
        .map_err(format_windows_service_error)?;
    wait_for_windows_service_state(&service, ServiceState::Running)
}

#[cfg(windows)]
fn uninstall_windows_service() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(format_windows_service_error)?;
    let service = manager
        .open_service(
            WINDOWS_SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(format_windows_service_error)?;

    stop_windows_service_if_needed(&service)?;
    service.delete().map_err(format_windows_service_error)
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
fn windows_service_is_active(state: ServiceState) -> bool {
    matches!(
        state,
        ServiceState::Running | ServiceState::StartPending | ServiceState::ContinuePending
    )
}

#[cfg(windows)]
fn windows_service_not_found(error: &WindowsServiceError) -> bool {
    match error {
        WindowsServiceError::Winapi(io_error) => {
            io_error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST)
        }
        _ => false,
    }
}

#[cfg(windows)]
fn format_windows_service_error(error: WindowsServiceError) -> String {
    match error {
        WindowsServiceError::Winapi(io_error) => match io_error.raw_os_error() {
            Some(ERROR_SERVICE_DOES_NOT_EXIST) => {
                "The background service is not installed.".to_string()
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

fn service_executable_path(_app_handle: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage_path) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage_path));
    }

    std::env::current_exe().map_err(|err| err.to_string())
}

fn write_temp_script(kind: &str, contents: &str) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!(
        "sdl-service-{kind}-{}.sh",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    fs::write(&path, contents).map_err(|err| err.to_string())?;
    Ok(path)
}

fn temp_plist_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}.plist",
        active_app_directory_name().replace(' ', "-")
    ))
}

#[cfg(any(target_os = "linux", windows))]
fn temp_result_path(kind: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sdl-{kind}-{}.txt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "linux")]
fn systemd_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(' ', "\\x20")
}

#[cfg(target_os = "macos")]
fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
