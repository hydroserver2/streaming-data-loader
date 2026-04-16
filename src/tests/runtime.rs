use super::{
    active_app_directory_name, copy_dir_contents, has_runtime_state, move_or_copy_dir_contents,
    preferred_user_data_dir, AppState, APP_DIRECTORY_NAME, DEV_APP_DIRECTORY_NAME,
};
use crate::models::{
    AuthType, ColumnMapping, FileConfig, IdentifierType, JobUpsertRequest, ServerConfig,
    TimestampConfig,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn copy_dir_contents_copies_nested_runtime_state() {
    let temp_root = unique_temp_dir("runtime-copy");
    let source = temp_root.join("source");
    let target = temp_root.join("target");

    fs::create_dir_all(source.join("workspaces")).expect("create source workspaces");
    fs::write(source.join("config.json"), "{}").expect("write config");
    fs::write(
        source.join("workspaces").join("workspace.json"),
        "{\"datasources\":[]}",
    )
    .expect("write workspace");

    copy_dir_contents(&source, &target).expect("copy runtime state");

    assert!(target.join("config.json").exists());
    assert!(target.join("workspaces").join("workspace.json").exists());

    remove_temp_dir(&temp_root);
}

#[test]
fn copy_dir_contents_does_not_overwrite_existing_files() {
    let temp_root = unique_temp_dir("runtime-preserve");
    let source = temp_root.join("source");
    let target = temp_root.join(APP_DIRECTORY_NAME);

    fs::create_dir_all(&source).expect("create source");
    fs::create_dir_all(&target).expect("create target");
    fs::write(source.join("config.json"), "{\"url\":\"new\"}").expect("write source");
    fs::write(target.join("config.json"), "{\"url\":\"existing\"}").expect("write target");

    copy_dir_contents(&source, &target).expect("copy runtime state");

    let persisted = fs::read_to_string(target.join("config.json")).expect("read target");
    assert_eq!(persisted, "{\"url\":\"existing\"}");

    remove_temp_dir(&temp_root);
}

#[test]
fn move_or_copy_dir_contents_moves_source_when_target_is_missing() {
    let temp_root = unique_temp_dir("runtime-move");
    let source = temp_root.join("source");
    let target = temp_root.join("target");

    fs::create_dir_all(&source).expect("create source");
    fs::write(source.join("config.json"), "{}").expect("write config");

    move_or_copy_dir_contents(&source, &target).expect("move runtime state");

    assert!(!source.exists());
    assert!(target.join("config.json").exists());

    remove_temp_dir(&temp_root);
}

#[test]
fn has_runtime_state_detects_config_or_workspace_dir() {
    let temp_root = unique_temp_dir("runtime-state");
    let config_only = temp_root.join("config-only");
    let workspace_only = temp_root.join("workspace-only");

    fs::create_dir_all(&config_only).expect("create config dir");
    fs::create_dir_all(workspace_only.join("workspaces")).expect("create workspace dir");
    fs::write(config_only.join("config.json"), "{}").expect("write config");

    assert!(has_runtime_state(&config_only));
    assert!(has_runtime_state(&workspace_only));
    assert!(!has_runtime_state(&temp_root.join("empty")));

    remove_temp_dir(&temp_root);
}

#[test]
fn active_app_directory_name_matches_build_mode() {
    let expected = if cfg!(debug_assertions) {
        DEV_APP_DIRECTORY_NAME
    } else {
        APP_DIRECTORY_NAME
    };

    assert_eq!(active_app_directory_name(), expected);
}

#[test]
fn preferred_user_data_dir_uses_app_data_dir_before_home_dir() {
    let temp_root = unique_temp_dir("runtime-app-data");
    let app_data_dir = temp_root.join("app-data").join("com.streaming-data-loader");
    let home_dir = temp_root.join("home");

    let resolved =
        preferred_user_data_dir(Some(app_data_dir.clone()), Some(home_dir)).expect("resolve dir");

    let expected = if cfg!(debug_assertions) {
        app_data_dir.join("dev")
    } else {
        app_data_dir
    };

    assert_eq!(resolved, expected);

    remove_temp_dir(&temp_root);
}

#[test]
fn preferred_user_data_dir_falls_back_to_home_dir_without_documents() {
    let temp_root = unique_temp_dir("runtime-home-fallback");
    let home_dir = temp_root.join("home");

    let resolved = preferred_user_data_dir(None, Some(home_dir.clone())).expect("resolve dir");

    assert_eq!(resolved, home_dir.join(active_app_directory_name()));

    remove_temp_dir(&temp_root);
}

#[test]
fn start_job_run_logs_initial_run_for_new_datasource() {
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

    std::thread::sleep(std::time::Duration::from_millis(50));
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
