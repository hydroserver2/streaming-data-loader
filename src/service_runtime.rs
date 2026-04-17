use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

use fs2::FileExt;
use tokio::{
    task::JoinHandle,
    time::{interval, Duration, MissedTickBehavior},
};

use crate::{
    config_store::ConfigStore, hydroserver::HydroServerService, pipeline::PipelineService,
    service_paths::resolve_shared_service_config_dir,
};

const CONFIG_POLL_INTERVAL: Duration = Duration::from_secs(2);
const PID_LOCK_FILENAME: &str = "daemon.pid";

pub fn run_daemon() -> Result<(), String> {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let config_dir = resolve_shared_service_config_dir()?;

    // Acquire an exclusive advisory lock on daemon.pid before doing anything
    // else. A second `streaming-data-loader --service` pointed at the same
    // shared config dir will fail here instead of fighting over cursor and
    // workspace state.
    //
    // TODO: This PID lock is the first half of the fix and solves the problem of 
    // a second user of a machine downloading the SDL and trying to spin up a 
    // second OS service. For now, this will block that creation since it's an 
    // unlikely use case. If two SDL instances is something we want to support at some point,
    // the second half is a daemon-owned HTTP RPC so the UI stops writing the
    // config dir directly — once the daemon is the sole writer, the remaining
    // cross-process races disappear. See project notes for the planned shape
    // (localhost HTTP + bearer token in <config_dir>/daemon.endpoint).
    let _pid_lock = acquire_daemon_pid_lock(&config_dir)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;

    runtime.block_on(async move {
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

#[cfg(test)]
#[path = "tests/service_runtime.rs"]
mod tests;
