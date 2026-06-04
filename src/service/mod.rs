#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{
    get_service_status, install_service, maybe_handle_service_management_cli, restart_service,
    uninstall_service,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{
    get_service_status, install_service, maybe_handle_service_management_cli, restart_service,
    uninstall_service,
};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::WINDOWS_SERVICE_NAME;
#[cfg(windows)]
pub use windows::{
    get_service_status, install_service, maybe_handle_service_management_cli, restart_service,
    uninstall_service,
};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub use unsupported::*;
