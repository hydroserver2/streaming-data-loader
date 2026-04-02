use std::{
    collections::HashMap,
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};

struct SidecarState(Mutex<Option<Child>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SidecarState(Mutex::new(None)))
        .setup(|app| {
            start_sidecar(app)?;
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

    if !cfg!(debug_assertions) {
        return Ok(());
    }

    let env_vars = read_env_file(&workspace_root().join(".env.development"))?;
    let host = env_vars
        .get("SDL_SIDECAR_HOST")
        .cloned()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = env_vars
        .get("SDL_SIDECAR_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5321);

    if port_is_in_use(&host, port) {
        return Ok(());
    }

    let child = Command::new("node")
        .arg("./scripts/run-sidecar.mjs")
        .current_dir(workspace_root())
        .envs(env_vars)
        .env("SDL_TAURI_MANAGED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    *app.state::<SidecarState>().0.lock().unwrap() = Some(child);

    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_env_file(path: &Path) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut env_vars = HashMap::new();

    if !path.exists() {
        return Ok(env_vars);
    }

    for line in fs::read_to_string(path)?.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            env_vars.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(env_vars)
}

fn port_is_in_use(host: &str, port: u16) -> bool {
    let address = format!("{host}:{port}");
    address
        .to_socket_addrs()
        .map(|addresses| {
            addresses.into_iter().any(|socket| {
                TcpStream::connect_timeout(&socket, Duration::from_millis(200)).is_ok()
            })
        })
        .unwrap_or(false)
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
                if let Some(mut child) = app.state::<SidecarState>().0.lock().unwrap().take() {
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
