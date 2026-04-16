use std::{
    ffi::OsString,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use sdl_core::runtime::AppState;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

const SERVICE_NAME: &str = "HydroServerSDL";

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> Result<(), String> {
    if std::env::args_os().any(|arg| arg == OsString::from("--console")) {
        return run_console_mode();
    }

    service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(|error| {
        format!(
            "failed to start Windows service dispatcher: {error}. Run sdl-service with --console when launching manually."
        )
    })?;

    Ok(())
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("{error}");
    }
}

fn run_console_mode() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(crate::run_console())
}

fn run_service() -> Result<(), String> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let shutdown_tx = Arc::new(Mutex::new(Some(shutdown_tx)));

    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control_event| match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Ok(mut slot) = shutdown_tx.lock() {
                    if let Some(sender) = slot.take() {
                        let _ = sender.send(());
                    }
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .map_err(|error| format!("failed to register service control handler: {error}"))?;

    set_service_status(
        &status_handle,
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
    )?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to build tokio runtime: {error}"))?;

    let result = runtime.block_on(run_service_loop(&status_handle, shutdown_rx));

    if let Err(error) = &result {
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(1),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        });
        return Err(error.clone());
    }

    set_service_status(
        &status_handle,
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
    )?;

    Ok(())
}

async fn run_service_loop(
    status_handle: &ServiceStatusHandle,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<(), String> {
    let config_dir = crate::prepare_service_directories()?;
    let _guard = crate::initialize_logging(&config_dir)?;

    tracing::info!(
        config_dir = %config_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        "sdl-service starting under Windows SCM"
    );

    let state = AppState::new(config_dir)
        .map_err(|error| format!("failed to initialize application state: {error}"))?;
    state
        .initialize_async()
        .await
        .map_err(|error| format!("failed to initialize pipeline: {error}"))?;

    set_service_status(
        status_handle,
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
    )?;

    tracing::info!("sdl-service ready; waiting for service stop");

    tokio::task::spawn_blocking(move || shutdown_rx.recv())
        .await
        .map_err(|error| format!("failed while waiting for Windows service stop signal: {error}"))?
        .map_err(|error| {
            format!("Windows service stop signal channel closed unexpectedly: {error}")
        })?;

    tracing::info!("Windows service stop requested; draining uploads");
    set_service_status(
        status_handle,
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
    )?;

    state.shutdown_async().await;
    tracing::info!("sdl-service stopped");
    Ok(())
}

fn set_service_status(
    status_handle: &ServiceStatusHandle,
    current_state: ServiceState,
    controls_accepted: ServiceControlAccept,
) -> Result<(), String> {
    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state,
            controls_accepted,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })
        .map_err(|error| format!("failed to update Windows service status: {error}"))
}
