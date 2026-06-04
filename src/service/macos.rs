use crate::models::ServiceStatusResponse;
use crate::service_paths::{active_app_directory_name, default_shared_service_config_dir};

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::AppHandle;

const SERVICE_LABEL: &str = "com.hydroserver.sdl";
const SERVICE_PLIST_PATH: &str = "/Library/LaunchDaemons/com.hydroserver.sdl.plist";

pub fn get_service_status() -> Result<ServiceStatusResponse, String> {
    let executable_path = service_executable_path()?;
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

pub fn install_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    let plist_contents = render_macos_plist()?;
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
    get_service_status()
}

pub fn restart_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
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
    get_service_status()
}

pub fn uninstall_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
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
    get_service_status()
}

pub fn maybe_handle_service_management_cli() -> Option<i32> {
    None
}

fn render_macos_plist() -> Result<String, String> {
    let executable_path = service_executable_path()?;
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

  <key>ThrottleInterval</key>
  <integer>10</integer>

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

fn service_executable_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|err| err.to_string())
}

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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
