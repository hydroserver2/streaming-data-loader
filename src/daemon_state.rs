use std::{path::PathBuf, sync::Arc};

use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{interval, Duration, MissedTickBehavior},
};

use crate::{
    csv_preview::preview_csv,
    models::{
        ActionResponse, AppBootstrapResponse, AppConfig, ConnectionTestResponse,
        CsvPreviewResponse, DaemonStatusSnapshot, DatastreamDetail, DatastreamSummary, JobDetail,
        JobLogsResponse, JobStatusSummary, JobUpsertRequest, LogLevel, ServerConfig,
        ServerUrlValidationResponse,
    },
    pipeline::PipelineService,
    runtime::AppState,
};

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(750);

#[derive(Clone)]
pub struct DaemonState {
    inner: Arc<DaemonStateInner>,
}

struct DaemonStateInner {
    app: AppState,
    pipeline: PipelineService,
    status_tx: watch::Sender<DaemonStatusSnapshot>,
}

impl DaemonState {
    pub async fn new(config_dir: PathBuf) -> Result<Self, String> {
        let app = AppState::new(config_dir)?;
        app.initialize()?;
        app.config_store().clear_all_running_jobs()?;

        let pipeline = PipelineService::new(app.config_store_handle(), app.hydroserver_handle());
        pipeline.initialize().await?;

        let snapshot = app.status_snapshot()?;
        let (status_tx, _) = watch::channel(snapshot);

        Ok(Self {
            inner: Arc::new(DaemonStateInner {
                app,
                pipeline,
                status_tx,
            }),
        })
    }

    pub fn subscribe_status(&self) -> watch::Receiver<DaemonStatusSnapshot> {
        self.inner.status_tx.subscribe()
    }

    pub fn clear_all_running_jobs(&self) -> Result<(), String> {
        self.inner.app.config_store().clear_all_running_jobs()
    }

    pub fn start_status_monitor(&self) -> JoinHandle<()> {
        let state = self.clone();
        tokio::spawn(async move {
            let mut ticker = interval(STATUS_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                let _ = state.publish_status();
            }
        })
    }

    pub async fn shutdown(&self) {
        self.inner.pipeline.shutdown().await;
    }

    pub fn bootstrap(&self) -> Result<AppBootstrapResponse, String> {
        self.inner.app.bootstrap()
    }

    pub fn health(&self) -> Result<crate::models::HealthResponse, String> {
        self.inner.app.health()
    }

    pub fn config(&self) -> Result<AppConfig, String> {
        self.inner.app.config()
    }

    pub fn jobs(&self) -> Result<Vec<JobStatusSummary>, String> {
        self.inner
            .app
            .config_store()
            .list_jobs()?
            .iter()
            .map(|job| self.inner.app.build_job_summary(job))
            .collect()
    }

    pub fn get_job(&self, job_id: &str) -> Result<JobDetail, String> {
        let Some(job) = self.inner.app.config_store().get_job(job_id)? else {
            return Err("That job could not be found.".to_string());
        };
        self.inner.app.build_job_detail(&job)
    }

    pub fn get_job_logs(&self, job_id: &str) -> Result<JobLogsResponse, String> {
        if self.inner.app.config_store().get_job(job_id)?.is_none() {
            return Err("That job could not be found.".to_string());
        }

        Ok(JobLogsResponse {
            entries: self.inner.app.config_store().logs_for(job_id, 200)?,
            log_file_path: self
                .inner
                .app
                .config_store()
                .job_log_file_path(job_id)?
                .map(|path| path.to_string_lossy().into_owned()),
        })
    }

    pub async fn update_server_config(&self, server: ServerConfig) -> Result<AppConfig, String> {
        let normalized = server.validated_for_connection()?;
        let connection = self
            .inner
            .app
            .hydroserver()
            .test_connection(&normalized)
            .await;
        if !connection.ok {
            return Err(connection.message);
        }

        let workspace_id = connection.workspace_id.unwrap_or_default();
        let workspace_name = connection
            .workspace_name
            .clone()
            .unwrap_or_else(|| normalized.workspace_name.clone());

        let config = self.inner.app.config_store().set_server(
            ServerConfig {
                workspace_id,
                workspace_name,
                ..normalized
            },
            connection.workspace_name.as_deref().unwrap_or_default(),
        )?;

        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        Ok(config)
    }

    pub async fn clear_server_config(&self) -> Result<AppConfig, String> {
        let config = self.inner.app.config_store().clear_server()?;
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        Ok(config)
    }

    pub async fn test_connection(
        &self,
        server: ServerConfig,
    ) -> Result<ConnectionTestResponse, String> {
        Ok(self
            .inner
            .app
            .hydroserver()
            .test_connection(&server.normalized())
            .await)
    }

    pub async fn validate_server_url(
        &self,
        url: String,
    ) -> Result<ServerUrlValidationResponse, String> {
        Ok(self.inner.app.hydroserver().validate_url(&url).await)
    }

    pub async fn create_job(&self, payload: JobUpsertRequest) -> Result<JobDetail, String> {
        let job = self.inner.app.config_store().create_job(payload)?;
        let _ = self
            .inner
            .app
            .append_log(&job.id, "Job created", LogLevel::Info);
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        self.inner.app.build_job_detail(&job)
    }

    pub async fn update_job(
        &self,
        job_id: &str,
        payload: JobUpsertRequest,
    ) -> Result<JobDetail, String> {
        let Some(job) = self.inner.app.config_store().update_job(job_id, payload)? else {
            return Err("That job could not be found.".to_string());
        };
        let _ = self
            .inner
            .app
            .append_log(&job.id, "Job updated", LogLevel::Info);
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        self.inner.app.build_job_detail(&job)
    }

    pub async fn delete_job(&self, job_id: &str) -> Result<ActionResponse, String> {
        if !self.inner.app.config_store().delete_job(job_id)? {
            return Err("That job could not be found.".to_string());
        }
        self.inner.app.config_store().delete_job_runtime(job_id)?;
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        Ok(ActionResponse {
            ok: true,
            message: "Job deleted.".to_string(),
        })
    }

    pub async fn run_job_now(&self, job_id: &str) -> Result<ActionResponse, String> {
        let job = self
            .inner
            .app
            .config_store()
            .get_job(job_id)?
            .ok_or_else(|| "That job could not be found.".to_string())?;

        if !job.enabled {
            return Err("Enable this data source before requesting a manual run.".to_string());
        }

        self.inner.pipeline.run_job_now(job_id).await?;
        self.inner
            .app
            .append_log(job_id, "Manual run requested", LogLevel::Info)?;
        self.publish_status()?;

        Ok(ActionResponse {
            ok: true,
            message: "Run requested.".to_string(),
        })
    }

    pub async fn enable_job(&self, job_id: &str) -> Result<ActionResponse, String> {
        let Some(job) = self
            .inner
            .app
            .config_store()
            .set_job_enabled(job_id, true)?
        else {
            return Err("That job could not be found.".to_string());
        };
        let _ = self
            .inner
            .app
            .append_log(&job.id, "Job enabled", LogLevel::Info);
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        Ok(ActionResponse {
            ok: true,
            message: "Job enabled.".to_string(),
        })
    }

    pub async fn disable_job(&self, job_id: &str) -> Result<ActionResponse, String> {
        let Some(job) = self
            .inner
            .app
            .config_store()
            .set_job_enabled(job_id, false)?
        else {
            return Err("That job could not be found.".to_string());
        };
        let _ = self
            .inner
            .app
            .append_log(&job.id, "Job disabled", LogLevel::Warning);
        self.inner.pipeline.reload().await?;
        self.publish_status()?;
        Ok(ActionResponse {
            ok: true,
            message: "Job disabled.".to_string(),
        })
    }

    pub async fn get_datastreams(&self) -> Result<Vec<DatastreamSummary>, String> {
        let config = self.inner.app.config()?;
        self.inner
            .app
            .hydroserver()
            .list_datastreams(&config.server)
            .await
            .map_err(|_| "Couldn't load datastreams from HydroServer right now.".to_string())
    }

    pub async fn get_datastream_detail(
        &self,
        datastream_id: &str,
    ) -> Result<DatastreamDetail, String> {
        let config = self.inner.app.config()?;
        self.inner
            .app
            .hydroserver()
            .get_datastream_detail(&config.server, datastream_id)
            .await
            .map_err(|_| {
                "Couldn't load datastream metadata from HydroServer right now.".to_string()
            })
    }

    pub fn get_csv_preview(
        &self,
        path: String,
        rows: Option<usize>,
    ) -> Result<CsvPreviewResponse, String> {
        let rows = rows.unwrap_or(100).clamp(1, 500);
        preview_csv(&path, rows)
    }

    pub fn publish_status(&self) -> Result<(), String> {
        let snapshot = self.inner.app.status_snapshot()?;
        let _ = self.inner.status_tx.send(snapshot);
        Ok(())
    }
}
