use std::{
    fs,
    path::{Path, PathBuf},
};

pub const APP_DIRECTORY_NAME: &str = "Streaming Data Loader";
pub const DEV_APP_DIRECTORY_NAME: &str = "Streaming Data Loader Dev";

const DAEMON_ENDPOINT_FILENAME: &str = "daemon.endpoint.json";

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

    #[cfg(target_os = "windows")]
    {
        let program_data = std::env::var("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\ProgramData"));
        let candidate = program_data.join(active_app_directory_name());
        fs::create_dir_all(&candidate).map_err(|err| err.to_string())?;
        return Ok(candidate);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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

pub fn daemon_endpoint_path(config_dir: &Path) -> PathBuf {
    config_dir.join(DAEMON_ENDPOINT_FILENAME)
}
