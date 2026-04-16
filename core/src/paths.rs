use std::path::PathBuf;

/// Returns the canonical service-level config directory for the current platform.
///
/// These paths are system-wide and accessible without a logged-in user session:
/// - macOS:   /Library/Application Support/HydroServerSDL/
/// - Windows: C:\ProgramData\HydroServerSDL\
/// - Linux:   /var/lib/hydroserver-sdl/
///
/// If the `SDL_CONFIG_DIR` environment variable is set, it takes precedence
/// over the platform default. This is useful for development and testing.
pub fn service_config_dir() -> Result<PathBuf, String> {
    if let Ok(override_dir) = std::env::var("SDL_CONFIG_DIR") {
        let path = PathBuf::from(override_dir);
        if path.as_os_str().is_empty() {
            return Err("SDL_CONFIG_DIR is set but empty.".to_string());
        }
        return Ok(path);
    }

    Ok(platform_service_config_dir())
}

#[cfg(target_os = "macos")]
fn platform_service_config_dir() -> PathBuf {
    PathBuf::from("/Library/Application Support/HydroServerSDL")
}

#[cfg(target_os = "windows")]
fn platform_service_config_dir() -> PathBuf {
    PathBuf::from(r"C:\ProgramData\HydroServerSDL")
}

#[cfg(target_os = "linux")]
fn platform_service_config_dir() -> PathBuf {
    PathBuf::from("/var/lib/hydroserver-sdl")
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_service_config_dir() -> PathBuf {
    // Fallback for unsupported platforms — callers should check SDL_CONFIG_DIR.
    PathBuf::from("/tmp/hydroserver-sdl")
}
