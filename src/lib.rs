mod commands;
mod config_store;
mod csv_preview;
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

use tauri::Manager;

use runtime::{resolve_config_dir, AppState};
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
        .setup(|app| {
            let state = AppState::new(resolve_config_dir(&app.handle())?)?;
            state.initialize()?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_health,
            commands::get_service_status,
            commands::get_config,
            commands::update_server_config,
            commands::clear_server_config,
            commands::test_connection,
            commands::validate_server_url,
            commands::get_jobs,
            commands::create_job,
            commands::get_job,
            commands::update_job,
            commands::delete_job,
            commands::run_job_now,
            commands::enable_job,
            commands::disable_job,
            commands::get_job_logs,
            commands::install_os_service,
            commands::restart_os_service,
            commands::uninstall_os_service,
            commands::get_datastreams,
            commands::get_datastream_detail,
            commands::get_csv_preview,
            commands::reveal_file_in_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
