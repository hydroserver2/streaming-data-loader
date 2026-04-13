use tauri::State;

use crate::{
    csv_preview::preview_csv,
    models::{
        ActionResponse, AppConfig, ConnectionTestResponse, CsvPreviewResponse, DatastreamSummary,
        HealthResponse, JobDetail, JobLogEntry, JobStatusSummary, JobUpsertRequest, ServerConfig,
        ServerUrlValidationResponse,
    },
    runtime::AppState,
};

#[tauri::command]
pub fn get_health(state: State<'_, AppState>) -> Result<HealthResponse, String> {
    state.health()
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    state.config()
}

#[tauri::command]
pub async fn update_server_config(
    server: ServerConfig,
    state: State<'_, AppState>,
) -> Result<AppConfig, String> {
    let normalized = server.validated_for_connection()?;
    let connection = state.hydroserver().test_connection(&normalized).await;
    if !connection.ok {
        return Err(connection.message);
    }

    let workspace_id = connection.workspace_id.unwrap_or_default();
    let config = state.config_store().set_server(
        ServerConfig {
            workspace_id,
            ..normalized
        },
        connection.workspace_name.as_deref().unwrap_or_default(),
    )?;
    state.reload_pipeline().await?;
    Ok(config)
}

#[tauri::command]
pub async fn clear_server_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config_store().clear_server()?;
    state.reload_pipeline().await?;
    Ok(config)
}

#[tauri::command]
pub async fn test_connection(
    server: ServerConfig,
    state: State<'_, AppState>,
) -> Result<ConnectionTestResponse, String> {
    Ok(state
        .hydroserver()
        .test_connection(&server.normalized())
        .await)
}

#[tauri::command]
pub async fn validate_server_url(
    url: String,
    state: State<'_, AppState>,
) -> Result<ServerUrlValidationResponse, String> {
    Ok(state.hydroserver().validate_url(&url).await)
}

#[tauri::command]
pub fn get_jobs(state: State<'_, AppState>) -> Result<Vec<JobStatusSummary>, String> {
    state
        .config_store()
        .list_jobs()?
        .iter()
        .map(|job| state.build_job_summary(job))
        .collect()
}

#[tauri::command]
pub async fn create_job(
    payload: JobUpsertRequest,
    state: State<'_, AppState>,
) -> Result<JobDetail, String> {
    let job = state.config_store().create_job(payload)?;
    let _ = state.append_log(&job.id, "Job created", crate::models::LogLevel::Info);
    state.reload_pipeline().await?;
    state.build_job_detail(&job)
}

#[tauri::command]
pub fn get_job(job_id: String, state: State<'_, AppState>) -> Result<JobDetail, String> {
    let Some(job) = state.config_store().get_job(&job_id)? else {
        return Err("That job could not be found.".to_string());
    };
    state.build_job_detail(&job)
}

#[tauri::command]
pub async fn update_job(
    job_id: String,
    payload: JobUpsertRequest,
    state: State<'_, AppState>,
) -> Result<JobDetail, String> {
    let Some(job) = state.config_store().update_job(&job_id, payload)? else {
        return Err("That job could not be found.".to_string());
    };
    let _ = state.append_log(&job.id, "Job updated", crate::models::LogLevel::Info);
    state.reload_pipeline().await?;
    state.build_job_detail(&job)
}

#[tauri::command]
pub async fn delete_job(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<ActionResponse, String> {
    if !state.config_store().delete_job(&job_id)? {
        return Err("That job could not be found.".to_string());
    }
    state.config_store().delete_job_runtime(&job_id)?;
    state.reload_pipeline().await?;
    Ok(ActionResponse {
        ok: true,
        message: "Job deleted.".to_string(),
    })
}

#[tauri::command]
pub fn run_job_now(job_id: String, state: State<'_, AppState>) -> Result<ActionResponse, String> {
    state.run_job_now(&job_id)
}

#[tauri::command]
pub async fn enable_job(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<ActionResponse, String> {
    let Some(job) = state.config_store().set_job_enabled(&job_id, true)? else {
        return Err("That job could not be found.".to_string());
    };
    let _ = state.append_log(&job.id, "Job enabled", crate::models::LogLevel::Info);
    state.reload_pipeline().await?;
    Ok(ActionResponse {
        ok: true,
        message: "Job enabled.".to_string(),
    })
}

#[tauri::command]
pub async fn disable_job(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<ActionResponse, String> {
    let Some(job) = state.config_store().set_job_enabled(&job_id, false)? else {
        return Err("That job could not be found.".to_string());
    };
    let _ = state.append_log(&job.id, "Job disabled", crate::models::LogLevel::Warning);
    state.reload_pipeline().await?;
    Ok(ActionResponse {
        ok: true,
        message: "Job disabled.".to_string(),
    })
}

#[tauri::command]
pub fn get_job_logs(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<JobLogEntry>, String> {
    if state.config_store().get_job(&job_id)?.is_none() {
        return Err("That job could not be found.".to_string());
    }
    state.config_store().logs_for(&job_id, 50)
}

#[tauri::command]
pub async fn get_datastreams(state: State<'_, AppState>) -> Result<Vec<DatastreamSummary>, String> {
    let config = state.config()?;
    state
        .hydroserver()
        .list_datastreams(&config.server)
        .await
        .map_err(|_| "Couldn't load datastreams from HydroServer right now.".to_string())
}

#[tauri::command]
pub fn get_csv_preview(path: String, rows: Option<usize>) -> Result<CsvPreviewResponse, String> {
    let rows = rows.unwrap_or(100).clamp(1, 500);
    preview_csv(&path, rows)
}
