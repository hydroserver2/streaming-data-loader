use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub const APP_DIRECTORY_NAME: &str = "Streaming Data Loader";
pub const DEV_APP_DIRECTORY_NAME: &str = "Streaming Data Loader Dev";
pub const SERVICE_CONFIG_DIR_FLAG: &str = "--service-config-dir";

const DAEMON_ENDPOINT_FILENAME: &str = "daemon.endpoint.json";

pub fn active_app_directory_name() -> &'static str {
    if cfg!(debug_assertions) {
        DEV_APP_DIRECTORY_NAME
    } else {
        APP_DIRECTORY_NAME
    }
}

pub fn resolve_shared_service_config_dir() -> Result<PathBuf, String> {
    if let Some(config_dir) = service_config_dir_override_from_args(std::env::args_os()) {
        fs::create_dir_all(&config_dir).map_err(|err| err.to_string())?;
        return Ok(config_dir);
    }

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

pub(crate) fn service_config_dir_override_from_args<I, T>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        if arg == OsString::from(SERVICE_CONFIG_DIR_FLAG) {
            return args.next().map(PathBuf::from);
        }
        if let Some(inline_path) = inline_service_config_dir_override(&arg) {
            return Some(inline_path);
        }
    }

    None
}

fn inline_service_config_dir_override(arg: &OsString) -> Option<PathBuf> {
    let prefix = format!("{SERVICE_CONFIG_DIR_FLAG}=");
    let value = arg.to_string_lossy();
    let raw_path = value.strip_prefix(&prefix)?;
    let path = raw_path.trim_matches('"');
    if path.is_empty() {
        return None;
    }

    Some(PathBuf::from(path))
}

#[cfg(test)]
#[path = "tests/service_paths.rs"]
mod tests;
