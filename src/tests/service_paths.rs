use super::{service_config_dir_override_from_args, SERVICE_CONFIG_DIR_FLAG};
use std::{ffi::OsString, path::PathBuf};

#[test]
fn service_config_dir_override_reads_following_argument() {
    let override_path = service_config_dir_override_from_args([
        OsString::from("streaming-data-loader.exe"),
        OsString::from("--service"),
        OsString::from(SERVICE_CONFIG_DIR_FLAG),
        OsString::from(r"C:\Projects\streaming-data-loader\.sdl-dev-data"),
    ]);

    assert_eq!(
        override_path,
        Some(PathBuf::from(
            r"C:\Projects\streaming-data-loader\.sdl-dev-data"
        ))
    );
}

#[test]
fn service_config_dir_override_ignores_missing_value() {
    let override_path = service_config_dir_override_from_args([
        OsString::from("streaming-data-loader.exe"),
        OsString::from(SERVICE_CONFIG_DIR_FLAG),
    ]);

    assert_eq!(override_path, None);
}
