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

use std::ffi::OsStr;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt as _;

use runtime::{resolve_config_dir, AppState};

const AUTOSTART_ARG: &str = "--autostart";
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let launched_via_autostart = launched_via_autostart();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_ARG)
                .app_name(autostart_app_name())
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let state = AppState::new(resolve_config_dir(&app.handle())?)?;
            state.initialize()?;
            app.manage(state);
            initialize_launch_at_login(&app.handle());
            setup_tray(app, launched_via_autostart)?;

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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(
    app: &mut tauri::App,
    launched_via_autostart: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Hide Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;

    TrayIconBuilder::new()
        .icon(Image::from_bytes(TRAY_ICON_BYTES)?)
        .icon_as_template(true)
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

    if !launched_via_autostart {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
    }

    Ok(())
}

fn initialize_launch_at_login<R: Runtime>(app_handle: &tauri::AppHandle<R>) {
    let should_initialize = match app_handle
        .state::<AppState>()
        .config_store()
        .mark_launch_at_login_initialized()
    {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(error = %error, "failed to persist launch-at-login initialization");
            false
        }
    };

    if !should_initialize {
        return;
    }

    let autostart_manager = app_handle.autolaunch();

    match autostart_manager.is_enabled() {
        Ok(true) => {}
        Ok(false) => {
            if let Err(error) = autostart_manager.enable() {
                tracing::warn!(error = %error, "failed to enable launch at login on first launch");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to read launch-at-login status");
        }
    }
}

fn launched_via_autostart() -> bool {
    has_launch_flag(std::env::args_os(), AUTOSTART_ARG)
}

fn has_launch_flag<I, S>(args: I, flag: &str) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter().any(|arg| arg.as_ref() == OsStr::new(flag))
}

fn autostart_app_name() -> &'static str {
    if cfg!(debug_assertions) {
        "Streaming Data Loader Dev"
    } else {
        "Streaming Data Loader"
    }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
