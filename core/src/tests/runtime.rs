use super::AppState;
use crate::models::{
    AuthType, ColumnMapping, FileConfig, IdentifierType, JobUpsertRequest, ServerConfig,
    TimestampConfig,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn start_job_run_logs_initial_run_for_new_datasource() {
    let temp_root = unique_temp_dir("runtime-start-job");
    let csv_path = temp_root.join("example.csv");
    fs::write(
        &csv_path,
        "\
Timestamp,Stage_ft
",
    )
    .expect("write csv");

    let state = AppState::new(temp_root.clone()).expect("create app state");
    state.config_store().ensure().expect("ensure config store");
    state
        .config_store()
        .set_server(
            ServerConfig {
                auth_type: AuthType::Apikey,
                url: "https://example.com".to_string(),
                api_key: "test-api-key".to_string(),
                workspace_id: "workspace-1".to_string(),
                workspace_name: "Test Workspace".to_string(),
                ..ServerConfig::default()
            },
            "Test Workspace",
        )
        .expect("set server");

    let job = state
        .config_store()
        .create_job(JobUpsertRequest {
            name: "Example".to_string(),
            enabled: true,
            file_path: csv_path.to_string_lossy().into_owned(),
            schedule_minutes: 15,
            file_config: FileConfig {
                header_row: Some(1),
                data_start_row: 2,
                delimiter: ",".to_string(),
                identifier_type: IdentifierType::Name,
                timestamp: TimestampConfig::default(),
            },
            column_mappings: vec![ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-1".to_string(),
                datastream_name: "Stage".to_string(),
            }],
        })
        .expect("create job");

    let started = state
        .start_job_run(&job.id, "Initial run started")
        .expect("start job");
    assert!(started);

    let logs = state
        .config_store()
        .logs_for(&job.id, 10)
        .expect("load logs for job");
    assert!(logs
        .iter()
        .any(|entry| entry.message == "Initial run started"));

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    remove_temp_dir(&temp_root);
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sdl-{label}-{nanos}"));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn remove_temp_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).expect("remove temp dir");
    }
}
