mod commands;
mod config_store;
mod csv_preview;
mod daemon_api;
mod daemon_launcher;
mod daemon_state;
mod file_watcher;
mod hydroserver;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    tauri::Builder::default()
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
