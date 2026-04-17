use std::{path::Path, process::Command};

use tauri::State;

use crate::{
    csv_preview::preview_csv,
    models::{
        ActionResponse, AppConfig, ConnectionTestResponse, CsvPreviewResponse, DatastreamDetail,
        DatastreamSummary, HealthResponse, JobDetail, JobLogsResponse, JobStatusSummary,
        JobUpsertRequest, ServerConfig, ServerUrlValidationResponse,
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
    let workspace_name = connection
        .workspace_name
        .clone()
        .unwrap_or_else(|| normalized.workspace_name.clone());
    state.config_store().set_server(
        ServerConfig {
            workspace_id,
            workspace_name,
            ..normalized
        },
        connection.workspace_name.as_deref().unwrap_or_default(),
    )
}

#[tauri::command]
pub async fn clear_server_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    state.config_store().clear_server()
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
    Ok(ActionResponse {
        ok: true,
        message: "Job deleted.".to_string(),
    })
}

#[tauri::command]
pub fn run_job_now(job_id: String, state: State<'_, AppState>) -> Result<ActionResponse, String> {
    state.request_job_run(&job_id, "Manual run requested")
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
    Ok(ActionResponse {
        ok: true,
        message: "Job disabled.".to_string(),
    })
}

#[tauri::command]
pub fn get_job_logs(job_id: String, state: State<'_, AppState>) -> Result<JobLogsResponse, String> {
    if state.config_store().get_job(&job_id)?.is_none() {
        return Err("That job could not be found.".to_string());
    }

    Ok(JobLogsResponse {
        entries: state.config_store().logs_for(&job_id, 200)?,
        log_file_path: state
            .config_store()
            .job_log_file_path(&job_id)?
            .map(|path| path.to_string_lossy().into_owned()),
    })
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
pub async fn get_datastream_detail(
    datastream_id: String,
    state: State<'_, AppState>,
) -> Result<DatastreamDetail, String> {
    let config = state.config()?;
    state
        .hydroserver()
        .get_datastream_detail(&config.server, &datastream_id)
        .await
        .map_err(|_| "Couldn't load datastream metadata from HydroServer right now.".to_string())
}

#[tauri::command]
pub fn get_csv_preview(path: String, rows: Option<usize>) -> Result<CsvPreviewResponse, String> {
    let rows = rows.unwrap_or(100).clamp(1, 500);
    preview_csv(&path, rows)
}

#[tauri::command]
pub fn reveal_file_in_folder(path: String) -> Result<ActionResponse, String> {
    let target = Path::new(&path);
    if !target.exists() {
        return Err("That file no longer exists.".to_string());
    }

    reveal_path_with_platform_file_manager(target)?;

    Ok(ActionResponse {
        ok: true,
        message: "Opened the file location.".to_string(),
    })
}

fn reveal_path_with_platform_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run_command(Command::new("open").arg("-R").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        let select_arg = format!("/select,{}", path.display());
        run_command(Command::new("explorer").arg(select_arg))
    }

    #[cfg(target_os = "linux")]
    {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent()
                .ok_or_else(|| "Couldn't determine the containing folder.".to_string())?
        };

        run_command(Command::new("xdg-open").arg(directory))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err("Opening files in the system file manager isn't supported on this OS.".to_string())
    }
}

fn run_command(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("The system file manager couldn't be opened.".to_string())
    }
}
