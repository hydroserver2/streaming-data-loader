use std::{fs, path::PathBuf};

use sdl_core::{paths::service_config_dir, runtime::AppState};

#[cfg(windows)]
mod platform;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    #[cfg(windows)]
    {
        return platform::windows::run();
    }

    #[cfg(not(windows))]
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(run_console())
    }
}

pub(crate) async fn run_console() -> Result<(), String> {
    let config_dir = prepare_service_directories()?;
    let _guard = initialize_logging(&config_dir)?;

    tracing::info!(
        config_dir = %config_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        "sdl-service starting"
    );

    let state = AppState::new(config_dir)
        .map_err(|error| format!("failed to initialize application state: {error}"))?;
    state
        .initialize_async()
        .await
        .map_err(|error| format!("failed to initialize pipeline: {error}"))?;

    tracing::info!("sdl-service ready; waiting for signals");

    wait_for_shutdown().await?;

    tracing::info!("shutdown signal received; draining uploads");
    state.shutdown_async().await;
    tracing::info!("sdl-service stopped");
    Ok(())
}

pub(crate) fn prepare_service_directories() -> Result<PathBuf, String> {
    let config_dir = service_config_dir()
        .map_err(|error| format!("failed to resolve service config directory: {error}"))?;
    fs::create_dir_all(&config_dir)
        .map_err(|error| format!("failed to create service config directory: {error}"))?;
    fs::create_dir_all(config_dir.join("logs"))
        .map_err(|error| format!("failed to create service logs directory: {error}"))?;
    Ok(config_dir)
}

pub(crate) fn initialize_logging(
    config_dir: &std::path::Path,
) -> Result<tracing_appender::non_blocking::WorkerGuard, String> {
    let logs_dir = config_dir.join("logs");
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "service.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(non_blocking)
        .try_init()
        .map_err(|error| error.to_string())?;
    Ok(guard)
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<(), String> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate())
        .map_err(|error| format!("failed to install SIGTERM handler: {error}"))?;
    let mut sigint = signal(SignalKind::interrupt())
        .map_err(|error| format!("failed to install SIGINT handler: {error}"))?;

    tokio::select! {
        _ = sigterm.recv() => {},
        _ = sigint.recv() => {},
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
async fn wait_for_shutdown() -> Result<(), String> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|error| format!("failed to install Ctrl-C handler: {error}"))
}

#[cfg(windows)]
async fn wait_for_shutdown() -> Result<(), String> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|error| format!("failed to install Ctrl-C handler: {error}"))
}
