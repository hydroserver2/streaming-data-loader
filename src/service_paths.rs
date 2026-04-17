use std::{
    fs,
    path::{Path, PathBuf},
};

pub const APP_DIRECTORY_NAME: &str = "Streaming Data Loader";
pub const DEV_APP_DIRECTORY_NAME: &str = "Streaming Data Loader Dev";

const MANUAL_TRIGGER_PREFIX: &str = ".sdl-run-now-";
const MANUAL_TRIGGER_SUFFIX: &str = ".trigger";

pub fn active_app_directory_name() -> &'static str {
    if cfg!(debug_assertions) {
        DEV_APP_DIRECTORY_NAME
    } else {
        APP_DIRECTORY_NAME
    }
}

pub fn resolve_shared_service_config_dir() -> Result<PathBuf, String> {
    if let Ok(config_dir) = std::env::var("SDL_CONFIG_DIR") {
        let candidate = PathBuf::from(config_dir);
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        return Ok(candidate);
    }

    default_shared_service_config_dir()
}

pub fn default_shared_service_config_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let candidate = PathBuf::from("/Users/Shared").join(active_app_directory_name());
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        return Ok(candidate);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .map_err(|_| "Couldn't resolve an application data directory.".to_string())?;
        let candidate = home_dir.join(active_app_directory_name());
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        Ok(candidate)
    }
}

pub fn manual_run_trigger_path(job_id: &str, file_path: &str) -> Result<PathBuf, String> {
    let watched_file = Path::new(file_path);
    let parent = watched_file
        .parent()
        .ok_or_else(|| "Couldn't determine the watched folder for this data source.".to_string())?;

    Ok(parent.join(format!(
        "{MANUAL_TRIGGER_PREFIX}{job_id}{MANUAL_TRIGGER_SUFFIX}"
    )))
}
