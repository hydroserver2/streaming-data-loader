use super::{
    active_shared_service_directory_name, service_config_dir_override_from_args,
    SERVICE_CONFIG_DIR_FLAG,
};
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

#[test]
fn service_config_dir_override_reads_inline_argument_with_spaces() {
    let override_path = service_config_dir_override_from_args([
        OsString::from("streaming-data-loader.exe"),
        OsString::from("--service"),
        OsString::from(r#"--service-config-dir="C:\ProgramData\Streaming Data Loader""#),
    ]);

    assert_eq!(
        override_path,
        Some(PathBuf::from(r"C:\ProgramData\Streaming Data Loader"))
    );
}

#[test]
fn service_config_dir_override_reads_inline_argument_without_quotes() {
    let override_path = service_config_dir_override_from_args([
        OsString::from("streaming-data-loader.exe"),
        OsString::from("--service"),
        OsString::from(r"--service-config-dir=C:\ProgramData\Streaming"),
    ]);

    assert_eq!(override_path, Some(PathBuf::from(r"C:\ProgramData\Streaming")));
}

#[cfg(windows)]
#[test]
fn active_shared_service_directory_name_uses_windows_safe_folder_name() {
    let expected = if cfg!(debug_assertions) {
        "StreamingDataLoaderDev"
    } else {
        "StreamingDataLoader"
    };

    assert_eq!(active_shared_service_directory_name(), expected);
}
