use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

const APP_DIRECTORY_NAME: &str = "Streaming Data Loader";
const DEV_APP_DIRECTORY_NAME: &str = "Streaming Data Loader Dev";
const BUNDLE_IDENTIFIER: &str = "com.streaming-data-loader";
const LEGACY_BUNDLE_IDENTIFIER: &str = "com.streaming-data-loader.app";

pub fn resolve_config_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(config_dir) = std::env::var("SDL_CONFIG_DIR") {
        let candidate = PathBuf::from(config_dir);
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        return Ok(candidate);
    }

    let preferred_dir = preferred_user_data_dir(
        app_handle.path().app_data_dir().ok(),
        app_handle.path().home_dir().ok(),
    )?;

    migrate_legacy_config_dir(app_handle, &preferred_dir)?;

    if try_create_dir(&preferred_dir) {
        return Ok(preferred_dir);
    }

    if let Ok(home_dir) = app_handle.path().home_dir() {
        let fallback_dir = home_dir.join(active_app_directory_name());
        migrate_legacy_config_dir(app_handle, &fallback_dir)?;
        fs::create_dir_all(&fallback_dir).map_err(|err| err.to_string())?;
        return Ok(fallback_dir);
    }

    Err("Couldn't resolve an application data directory.".to_string())
}

fn preferred_user_data_dir(
    app_data_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(app_data_dir) = app_data_dir {
        return Ok(if cfg!(debug_assertions) {
            app_data_dir.join("dev")
        } else {
            app_data_dir
        });
    }

    if let Some(home_dir) = home_dir {
        return Ok(home_dir.join(active_app_directory_name()));
    }

    Err("Couldn't resolve an application data directory.".to_string())
}

fn try_create_dir(path: &Path) -> bool {
    fs::create_dir_all(path).is_ok()
}

fn migrate_legacy_config_dir(app_handle: &AppHandle, target_dir: &Path) -> Result<(), String> {
    if has_runtime_state(target_dir) {
        return Ok(());
    }

    let Some(source_dir) = legacy_config_candidates(app_handle)
        .into_iter()
        .find(|candidate| candidate != target_dir && has_runtime_state(candidate))
    else {
        return Ok(());
    };

    move_or_copy_dir_contents(&source_dir, target_dir)
}

fn active_app_directory_name() -> &'static str {
    if cfg!(debug_assertions) {
        DEV_APP_DIRECTORY_NAME
    } else {
        APP_DIRECTORY_NAME
    }
}

fn legacy_config_candidates(app_handle: &AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(data_dir) = app_handle.path().data_dir() {
        candidates.push(data_dir.join(LEGACY_BUNDLE_IDENTIFIER));
        candidates.push(data_dir.join(BUNDLE_IDENTIFIER));
    }

    if let Ok(document_dir) = app_handle.path().document_dir() {
        candidates.push(document_dir.join(APP_DIRECTORY_NAME));
        if cfg!(debug_assertions) {
            candidates.push(document_dir.join(DEV_APP_DIRECTORY_NAME));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("Streaming Data Loader Data"));
    }

    if let Ok(home_dir) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let home_dir = PathBuf::from(home_dir);
        candidates.push(home_dir.join(APP_DIRECTORY_NAME));
        if cfg!(debug_assertions) {
            candidates.push(home_dir.join(DEV_APP_DIRECTORY_NAME));
        }
    }

    candidates
}

fn has_runtime_state(path: &Path) -> bool {
    path.join("config.json").exists() || path.join("workspaces").is_dir()
}

fn copy_dir_contents(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|err| err.to_string())?;

    for entry in fs::read_dir(source_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_contents(&source_path, &target_path)?;
        } else if source_path.is_file() && !target_path.exists() {
            fs::copy(&source_path, &target_path).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn move_or_copy_dir_contents(source_dir: &Path, target_dir: &Path) -> Result<(), String> {
    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    if !target_dir.exists() && fs::rename(source_dir, target_dir).is_ok() {
        return Ok(());
    }

    copy_dir_contents(source_dir, target_dir)
}
