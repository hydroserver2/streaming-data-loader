use crate::models::ServiceStatusResponse;

use tauri::AppHandle;

pub fn get_service_status() -> Result<ServiceStatusResponse, String> {
    Ok(unsupported_service_status(
        "Background service management is only available on macOS, Windows, and Linux systemd hosts.",
    ))
}

pub fn install_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    Err("Background service management isn't supported on this OS.".to_string())
}

pub fn restart_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    Err("Background service management isn't supported on this OS.".to_string())
}

pub fn uninstall_service(_app_handle: &AppHandle) -> Result<ServiceStatusResponse, String> {
    Err("Background service management isn't supported on this OS.".to_string())
}

pub fn maybe_handle_service_management_cli() -> Option<i32> {
    None
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
