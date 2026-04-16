mod commands;
mod config_store;
mod csv_preview;
mod file_watcher;
mod hydroserver;
mod models;
mod observation_queue;
mod pipeline;
mod runtime;
mod timestamp;
mod uploader;

use tauri::Manager;

use runtime::{resolve_config_dir, AppState};

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

            // Graceful shutdown on SIGTERM / SIGINT so the uploader can drain
            // any queued observations before the process exits.
            // NOTE: must call shutdown_async() — not shutdown() — because block_on
            // panics when called from inside an async task.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let (mut sigterm, mut sigint) = match (
                        signal(SignalKind::terminate()),
                        signal(SignalKind::interrupt()),
                    ) {
                        (Ok(t), Ok(i)) => (t, i),
                        (Err(e), _) | (_, Err(e)) => {
                            tracing::error!(error = %e, "failed to install OS signal handlers");
                            return;
                        }
                    };
                    tokio::select! {
                        _ = sigterm.recv() => {},
                        _ = sigint.recv() => {},
                    }
                }
                #[cfg(not(unix))]
                {
                    if let Err(e) = tokio::signal::ctrl_c().await {
                        tracing::error!(error = %e, "failed to install Ctrl-C handler");
                        return;
                    }
                }
                app_handle.state::<AppState>().shutdown_async().await;
                app_handle.exit(0);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_health,
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
            commands::get_datastreams,
            commands::get_datastream_detail,
            commands::get_csv_preview,
            commands::reveal_file_in_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
