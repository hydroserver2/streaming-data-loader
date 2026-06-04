use std::{
    fs::{File, OpenOptions},
    future::Future,
    io::{Seek, SeekFrom, Write},
    path::Path,
};

use crate::{
    daemon_api::DaemonApiServer, daemon_state::DaemonState,
    service_paths::resolve_shared_service_config_dir,
};
use fs2::FileExt;

#[cfg(windows)]
use std::{ffi::OsString, sync::mpsc, time::Duration};

#[cfg(windows)]
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher, Error as WindowsServiceError,
};

const PID_LOCK_FILENAME: &str = "daemon.pid";

#[cfg(windows)]
const WINDOWS_SERVICE_DISPATCHER_CONNECT_ERROR: i32 = 1063;
#[cfg(windows)]
const WINDOWS_SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

#[cfg(windows)]
define_windows_service!(ffi_windows_service_main, windows_service_main);

pub fn run_daemon() -> Result<(), String> {
    crate::logging::init_daemon_logging();

    #[cfg(windows)]
    {
        if let Some(result) = try_run_under_windows_service_manager() {
            return result;
        }
    }

    run_console_daemon()
}

type ReadyCallback = Box<dyn FnOnce() + Send>;

fn run_console_daemon() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;

    runtime.block_on(run_daemon_until(wait_for_shutdown_signal(), None))
}

async fn run_daemon_until<F>(shutdown: F, on_ready: Option<ReadyCallback>) -> Result<(), String>
where
    F: Future<Output = Result<(), String>>,
{
    let config_dir = resolve_shared_service_config_dir()?;

    let _pid_lock = acquire_daemon_pid_lock(&config_dir)?;

    tracing::info!(config_dir = %config_dir.display(), "starting SDL daemon");

    let daemon = DaemonState::new(config_dir.clone()).await?;
    daemon.clear_all_running_jobs()?;
    daemon.publish_status()?;
    let status_task = daemon.start_status_monitor();
    let api_server = DaemonApiServer::start(daemon.clone(), config_dir).await?;

    if let Some(on_ready) = on_ready {
        on_ready();
    }

    let shutdown_result = shutdown.await;

    status_task.abort();
    let _ = status_task.await;
    api_server.shutdown().await;
    daemon.shutdown().await;
    daemon.clear_all_running_jobs()?;

    tracing::info!("SDL daemon stopped");
    shutdown_result
}

async fn wait_for_shutdown_signal() -> Result<(), String> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let (mut sigterm, mut sigint) = match (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) {
            (Ok(term), Ok(int)) => (term, int),
            (Err(error), _) | (_, Err(error)) => {
                return Err(format!("failed to install OS signal handlers: {error}"));
            }
        };

        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }

        Ok(())
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .map_err(|error| format!("failed to install Ctrl-C handler: {error}"))
    }

    #[cfg(not(any(unix, windows)))]
    {
        tokio::signal::ctrl_c()
            .await
            .map_err(|error| format!("failed to install Ctrl-C handler: {error}"))
    }
}

fn acquire_daemon_pid_lock(config_dir: &Path) -> Result<File, String> {
    let pid_path = config_dir.join(PID_LOCK_FILENAME);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&pid_path)
        .map_err(|err| {
            format!(
                "Couldn't open daemon pid file at {}: {err}",
                pid_path.display()
            )
        })?;

    FileExt::try_lock_exclusive(&file).map_err(|_| {
        format!(
            "Another streaming-data-loader daemon is already running (lock held at {}). \
             If this is stale, stop the service and delete the file before restarting.",
            pid_path.display()
        )
    })?;

    // Overwrite the file contents with the current PID. Best-effort — the lock
    // itself is what enforces single-instance; the PID is informational.
    let _ = file.set_len(0);
    let _ = (&file).seek(SeekFrom::Start(0));
    let _ = writeln!(&file, "{}", std::process::id());

    Ok(file)
}

#[cfg(windows)]
fn try_run_under_windows_service_manager() -> Option<Result<(), String>> {
    match service_dispatcher::start(
        crate::service::WINDOWS_SERVICE_NAME,
        ffi_windows_service_main,
    ) {
        Ok(()) => Some(Ok(())),
        Err(WindowsServiceError::Winapi(error))
            if error.raw_os_error() == Some(WINDOWS_SERVICE_DISPATCHER_CONNECT_ERROR) =>
        {
            None
        }
        Err(error) => Some(Err(format!(
            "Couldn't attach to the Windows Service Control Manager: {error}"
        ))),
    }
}

#[cfg(windows)]
fn windows_service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_windows_service() {
        tracing::error!(error = %error, "Windows service stopped with an error");
    }
}

#[cfg(windows)]
fn run_windows_service() -> Result<(), String> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle =
        service_control_handler::register(crate::service::WINDOWS_SERVICE_NAME, event_handler)
            .map_err(|error| error.to_string())?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: WINDOWS_SERVICE_TYPE,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::NO_ERROR,
            checkpoint: 1,
            wait_hint: Duration::from_secs(30),
            process_id: None,
        })
        .map_err(|error| error.to_string())?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;

    let ready_status_handle = status_handle;
    let on_ready: ReadyCallback = Box::new(move || {
        if let Err(error) = ready_status_handle.set_service_status(ServiceStatus {
            service_type: WINDOWS_SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::NO_ERROR,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        }) {
            tracing::error!(error = %error, "Couldn't transition Windows service to Running");
        }
    });

    let result = runtime.block_on(run_daemon_until(
        async move {
            tokio::task::spawn_blocking(move || shutdown_rx.recv())
                .await
                .map_err(|err| err.to_string())?
                .map_err(|_| "The Windows service control channel disconnected.".to_string())?;
            Ok(())
        },
        Some(on_ready),
    ));

    let exit_code = if result.is_ok() {
        ServiceExitCode::NO_ERROR
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };

    status_handle
        .set_service_status(ServiceStatus {
            service_type: WINDOWS_SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|error| error.to_string())?;

    result
}

#[cfg(test)]
#[path = "tests/service_runtime.rs"]
mod tests;
