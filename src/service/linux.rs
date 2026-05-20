use crate::models::ServiceStatusResponse;

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tauri::AppHandle;

const SERVICE_NAME: &str = "streaming-data-loader.service";
const SERVICE_DISPLAY_NAME: &str = "Streaming Data Loader";
const SERVICE_PATH: &str = "/etc/systemd/system/streaming-data-loader.service";
const SERVICE_ACTION_FLAG: &str = "--linux-service-action";
const SERVICE_RESULT_FLAG: &str = "--linux-service-result-file";
const SERVICE_USER_FLAG: &str = "--linux-service-user";
const SERVICE_HOME_FLAG: &str = "--linux-service-home";
const SERVICE_CONFIG_FLAG: &str = "--linux-service-config-dir";
const SERVICE_EXEC_FLAG: &str = "--linux-service-exec-path";

pub fn get_service_status() -> Result<ServiceStatusResponse, String> {
    if !linux_systemd_supported()? {
        return Ok(unsupported_service_status(
            "Background service management requires a Linux systemd host with systemctl available.",
        ));
    }

    let properties = query_service_properties()?;
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
        .unwrap_or_else(|| SERVICE_PATH.to_string());

    let installed = Path::new(SERVICE_PATH).exists()
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
        label: SERVICE_DISPLAY_NAME.to_string(),
        plist_path: fragment_path,
        executable_path: String::new(),
        status_message,
    })
}

pub fn install_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_linux_elevated_action(app_handle, "install")?;
    get_service_status()
}

pub fn restart_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_linux_elevated_action(app_handle, "restart")?;
    get_service_status()
}

pub fn uninstall_service(app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    run_linux_elevated_action(app_handle, "uninstall")?;
    get_service_status()
}

pub fn maybe_handle_service_management_cli() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let mut action: Option<OsString> = None;
    let mut result_file: Option<PathBuf> = None;
    let mut user: Option<OsString> = None;
    let mut home: Option<OsString> = None;
    let mut config_dir: Option<PathBuf> = None;
    let mut exec_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new(SERVICE_ACTION_FLAG) {
            action = args.next();
        } else if arg == OsStr::new(SERVICE_RESULT_FLAG) {
            result_file = args.next().map(PathBuf::from);
        } else if arg == OsStr::new(SERVICE_USER_FLAG) {
            user = args.next();
        } else if arg == OsStr::new(SERVICE_HOME_FLAG) {
            home = args.next();
        } else if arg == OsStr::new(SERVICE_CONFIG_FLAG) {
            config_dir = args.next().map(PathBuf::from);
        } else if arg == OsStr::new(SERVICE_EXEC_FLAG) {
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

struct LinuxServiceContext {
    user: Option<OsString>,
    home: Option<OsString>,
    config_dir: Option<PathBuf>,
    exec_path: Option<PathBuf>,
}

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
    fs::write(SERVICE_PATH, unit_contents).map_err(|err| err.to_string())?;

    run_systemctl(&["daemon-reload"])?;
    run_systemctl(&["enable", "--now", SERVICE_NAME])?;
    Ok(())
}

fn restart_linux_service() -> Result<(), String> {
    if !Path::new(SERVICE_PATH).exists() {
        return Err("The background service is not installed.".to_string());
    }

    run_systemctl(&["restart", SERVICE_NAME])
}

fn uninstall_linux_service() -> Result<(), String> {
    if Path::new(SERVICE_PATH).exists() {
        let _ = run_systemctl(&["disable", "--now", SERVICE_NAME]);
        let _ = run_systemctl(&["reset-failed", SERVICE_NAME]);
        fs::remove_file(SERVICE_PATH).map_err(|err| err.to_string())?;
        run_systemctl(&["daemon-reload"])?;
    }

    Ok(())
}

// === Helper functions ===

fn query_service_properties() -> Result<std::collections::HashMap<String, String>, String> {
    let output = Command::new("systemctl")
        .args([
            "show",
            SERVICE_NAME,
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

fn service_executable_path() -> Result<PathBuf, String> {
    if let Some(appimage_path) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage_path));
    }

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

fn run_linux_elevated_action(app_handle: &AppHandle, action: &str) -> Result<(), String> {
    let executable_path = service_executable_path()?;
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
        .arg(SERVICE_ACTION_FLAG)
        .arg(action)
        .arg(SERVICE_RESULT_FLAG)
        .arg(&result_path)
        .arg(SERVICE_USER_FLAG)
        .arg(&user)
        .arg(SERVICE_HOME_FLAG)
        .arg(&home)
        .arg(SERVICE_CONFIG_FLAG)
        .arg(&config_dir)
        .arg(SERVICE_EXEC_FLAG)
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

fn run_linux_management_action(action: &OsStr, context: LinuxServiceContext) -> Result<(), String> {
    match action.to_string_lossy().as_ref() {
        "install" => install_linux_service(context),
        "restart" => restart_linux_service(),
        "uninstall" => uninstall_linux_service(),
        _ => Err("Unknown Linux service action.".to_string()),
    }
}

fn render_linux_unit(user: &str, home: &str, config_dir: &Path, exec_path: &Path) -> String {
    format!(
        "[Unit]\nDescription={display_name}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser={user}\nWorkingDirectory={working_dir}\nEnvironment=\"HOME={home}\"\nEnvironment=\"SDL_CONFIG_DIR={config_dir}\"\nExecStart=\"{exec_path}\" --service\nRestart=always\nRestartSec=2\n\n[Install]\nWantedBy=multi-user.target\n",
        display_name = SERVICE_DISPLAY_NAME,
        user = systemd_escape(user),
        working_dir = systemd_escape(&config_dir.to_string_lossy()),
        home = systemd_escape(home),
        config_dir = systemd_escape(&config_dir.to_string_lossy()),
        exec_path = systemd_escape(&exec_path.to_string_lossy()),
    )
}

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

fn parse_systemctl_properties(output: &str) -> std::collections::HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

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

fn systemd_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(' ', "\\x20")
}
