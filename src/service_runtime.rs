use std::sync::Arc;

use tokio::{
    task::JoinHandle,
    time::{interval, Duration, MissedTickBehavior},
};

use crate::{
    config_store::ConfigStore, hydroserver::HydroServerService, pipeline::PipelineService,
    service_paths::resolve_shared_service_config_dir,
};

const CONFIG_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub fn run_daemon() -> Result<(), String> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;

    runtime.block_on(async {
        let config_dir = resolve_shared_service_config_dir()?;
        tracing::info!(config_dir = %config_dir.display(), "starting SDL daemon");

        let config_store = Arc::new(ConfigStore::new(config_dir));
        config_store.ensure()?;
        config_store.clear_all_running_jobs()?;

        let hydroserver = Arc::new(HydroServerService::new()?);
        let pipeline = PipelineService::new(config_store.clone(), hydroserver);
        pipeline.initialize().await?;

        let config_task = spawn_config_monitor(
            config_store.clone(),
            pipeline.clone(),
            config_store.watch_config_signature()?,
        );

        wait_for_shutdown_signal().await?;

        config_task.abort();
        let _ = config_task.await;
        pipeline.shutdown().await;
        config_store.clear_all_running_jobs()?;

        tracing::info!("SDL daemon stopped");
        Ok(())
    })
}

fn spawn_config_monitor(
    config_store: Arc<ConfigStore>,
    pipeline: PipelineService,
    initial_signature: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(CONFIG_POLL_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current_signature = initial_signature;

        loop {
            ticker.tick().await;

            let next_signature = match config_store.watch_config_signature() {
                Ok(signature) => signature,
                Err(error) => {
                    tracing::error!(error = %error, "failed to read persisted config state");
                    continue;
                }
            };

            if next_signature == current_signature {
                continue;
            }

            tracing::info!("persisted config changed; reloading daemon watch plan");
            match pipeline.reload().await {
                Ok(()) => current_signature = next_signature,
                Err(error) => {
                    tracing::error!(error = %error, "failed to reload daemon watch plan");
                }
            }
        }
    })
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

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .map_err(|error| format!("failed to install Ctrl-C handler: {error}"))
    }
}
