mod commands;
mod config_store;
mod csv_preview;
mod daemon_api;
mod daemon_launcher;
mod daemon_state;
mod file_watcher;
mod hydroserver;
mod logging;
mod models;
mod observation_queue;
mod pipeline;
mod runtime;
mod service_manager;
mod service_paths;
mod service_runtime;
mod timestamp;
mod uploader;
pub use service_manager::maybe_handle_service_management_cli;
pub use service_runtime::run_daemon;
pub use logging::init_process_logging_from_args;

#[cfg(windows)]
use tauri::Manager;

#[cfg(windows)]
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DwmSetWindowAttribute,
};

#[cfg(windows)]
const WINDOW_CHROME_DARK_BACKGROUND_RGB: u32 = 0x33312f;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init_desktop_logging();

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = apply_windows_chrome_color(&window) {
                    tracing::warn!(error = %error, "Couldn't apply the Windows chrome color override");
                }
            }

            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_connection,
            commands::get_service_status,
            commands::install_os_service,
            commands::restart_os_service,
            commands::uninstall_os_service,
            commands::reveal_file_in_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}

#[cfg(windows)]
fn apply_windows_chrome_color(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let chrome_color = WINDOW_CHROME_DARK_BACKGROUND_RGB;

    // DWM expects COLORREF in 0x00bbggrr order, so this constant matches #2f3133.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &chrome_color as *const _ as _,
            std::mem::size_of_val(&chrome_color) as u32,
        )
    }
    .map_err(|error| error.to_string())?;

    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &chrome_color as *const _ as _,
            std::mem::size_of_val(&chrome_color) as u32,
        )
    }
    .map_err(|error| error.to_string())
}
