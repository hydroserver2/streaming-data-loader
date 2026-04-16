use std::fs;

use sdl_core::{paths::service_config_dir, runtime::AppState};

#[tokio::main]
async fn main() {
    let config_dir = service_config_dir().expect("failed to resolve service config directory");
    fs::create_dir_all(&config_dir).expect("failed to create service config directory");

    let logs_dir = config_dir.join("logs");
    fs::create_dir_all(&logs_dir).expect("failed to create service logs directory");

    let file_appender = tracing_appender::rolling::daily(&logs_dir, "service.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(non_blocking)
        .init();

    tracing::info!(
        config_dir = %config_dir.display(),
        version = env!("CARGO_PKG_VERSION"),
        "sdl-service starting"
    );

    let state = AppState::new(config_dir).expect("failed to initialize application state");
    state
        .initialize_async()
        .await
        .expect("failed to initialize pipeline");

    tracing::info!("sdl-service ready; waiting for signals");

    wait_for_shutdown().await;

    tracing::info!("shutdown signal received; draining uploads");
    state.shutdown_async().await;
    tracing::info!("sdl-service stopped");
}

#[cfg(unix)]
async fn wait_for_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {},
        _ = sigint.recv() => {},
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
}
