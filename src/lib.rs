use std::sync::Mutex;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;

struct SidecarState(Mutex<Option<CommandChild>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(SidecarState(Mutex::new(None)))
        .setup(|app| {
            // start_sidecar(app)?;
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn start_sidecar(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;

    let sidecar = app.shell().sidecar("sidecar")?;
    let (_rx, child) = sidecar.spawn()?;

    *app.state::<SidecarState>().0.lock().unwrap() = Some(child);

    Ok(())
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        image::Image,
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        Manager,
    };

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
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide().unwrap();
                }
            }
            "quit" => {
                // Kill the sidecar before quitting
                if let Some(child) = app.state::<SidecarState>().0.lock().unwrap().take() {
                    let _ = child.kill();
                }
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
                        window.hide().unwrap();
                    } else {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
            }
        })
        .build(app)?;

    if let Some(window) = app.get_webview_window("main") {
        window.show().unwrap();
    }

    Ok(())
}