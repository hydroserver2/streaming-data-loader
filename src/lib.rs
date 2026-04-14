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

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

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
            setup_tray(app)?;

            // Graceful shutdown on SIGTERM / SIGINT so the uploader can drain
            // any queued observations before the process exits.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sigterm =
                        signal(SignalKind::terminate()).expect("SIGTERM handler failed");
                    let mut sigint =
                        signal(SignalKind::interrupt()).expect("SIGINT handler failed");
                    tokio::select! {
                        _ = sigterm.recv() => {},
                        _ = sigint.recv() => {},
                    }
                }
                #[cfg(not(unix))]
                {
                    tokio::signal::ctrl_c()
                        .await
                        .expect("Ctrl-C handler failed");
                }
                app_handle.state::<AppState>().shutdown();
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
            commands::get_csv_preview,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

    TrayIconBuilder::new()
        .icon(Image::from_path("icons/32x32.png")?)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                app.state::<AppState>().shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }

    Ok(())
}
