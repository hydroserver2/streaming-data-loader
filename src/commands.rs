use std::{path::Path, process::Command};

use tauri::AppHandle;

use crate::{
    daemon_launcher,
    models::{ActionResponse, DaemonConnectionInfo, ServiceStatusResponse},
    service,
};

#[tauri::command]
pub async fn get_daemon_connection(app: AppHandle) -> Result<DaemonConnectionInfo, String> {
    daemon_launcher::ensure_daemon_connection(&app).await
}

#[tauri::command]
pub fn get_service_status(_app: AppHandle) -> Result<ServiceStatusResponse, String> {
    service::get_service_status()
}

#[tauri::command]
pub fn install_os_service(app: AppHandle) -> Result<ServiceStatusResponse, String> {
    service::install_service(&app)
}

#[tauri::command]
pub fn restart_os_service(app: AppHandle) -> Result<ServiceStatusResponse, String> {
    service::restart_service(&app)
}

#[tauri::command]
pub fn uninstall_os_service(app: AppHandle) -> Result<ServiceStatusResponse, String> {
    service::uninstall_service(&app)
}

#[tauri::command]
pub fn reveal_file_in_folder(path: String) -> Result<ActionResponse, String> {
    let target = Path::new(&path);
    if !target.exists() {
        return Err("That file no longer exists.".to_string());
    }

    reveal_path_with_platform_file_manager(target)?;

    Ok(ActionResponse {
        ok: true,
        message: "Opened the file location.".to_string(),
    })
}

fn reveal_path_with_platform_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run_command(Command::new("open").arg("-R").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        let select_arg = format!("/select,{}", path.display());
        run_command(Command::new("explorer").arg(select_arg))
    }

    #[cfg(target_os = "linux")]
    {
        let directory = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };
        run_command(Command::new("xdg-open").arg(directory))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err("Opening file locations is not supported on this operating system.".to_string())
    }
}

fn run_command(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("The operating system could not open that file location.".to_string())
    }
}
