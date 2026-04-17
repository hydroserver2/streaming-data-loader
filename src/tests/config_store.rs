use super::ConfigStore;
use crate::models::{
    ColumnMapping, FileConfig, JobLogEntry, JobUpsertRequest, LogLevel, ServerConfig,
};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn append_log_persists_to_job_log_file_without_growing_workspace_json() {
    let temp_dir = unique_temp_dir("config-store-logs");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");
    store
        .set_server(
            ServerConfig {
                url: "https://example.com".to_string(),
                workspace_id: "workspace-1".to_string(),
                workspace_name: "Workspace 1".to_string(),
                ..ServerConfig::default()
            },
            "Workspace 1",
        )
        .expect("set server");

    let job = store
        .create_job(JobUpsertRequest {
            name: "Test source".to_string(),
            enabled: true,
            file_path: "/tmp/source.csv".to_string(),
            schedule_minutes: 15,
            file_config: FileConfig::default(),
            column_mappings: Vec::new(),
        })
        .expect("create job");

    let first = JobLogEntry {
        timestamp: Utc::now(),
        level: LogLevel::Info,
        message: "first message".to_string(),
    };
    let second = JobLogEntry {
        timestamp: Utc::now(),
        level: LogLevel::Warning,
        message: "second message".to_string(),
    };

    store
        .append_log(&job.id, first.clone())
        .expect("append first log");
    store
        .append_log(&job.id, second.clone())
        .expect("append second log");

    let log_path = store
        .job_log_file_path(&job.id)
        .expect("get log file path")
        .expect("job log file should exist");
    let logs = store.logs_for(&job.id, 200).expect("load logs");

    assert_eq!(logs, vec![first, second]);
    assert!(log_path.exists());

    let workspace_contents =
        fs::read_to_string(temp_dir.join("workspaces").join("workspace-1.json"))
            .expect("read workspace file");
    assert!(
        !workspace_contents.contains("recent_logs"),
        "workspace JSON should not embed recent_logs once file-backed logging is enabled"
    );

    remove_temp_dir(&temp_dir);
}

#[test]
fn loading_workspace_migrates_embedded_recent_logs_to_file_backed_logs() {
    let temp_dir = unique_temp_dir("config-store-migration");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");

    fs::write(
        temp_dir.join("config.json"),
        r#"{
  "version": 1,
  "server": {
    "auth_type": "apikey",
    "url": "https://example.com",
    "api_key": "",
    "username": "",
    "password": "",
    "workspace_id": "workspace-2",
    "workspace_name": "Workspace 2"
  }
}
"#,
    )
    .expect("write config");

    let migrated_entry = JobLogEntry {
        timestamp: Utc::now(),
        level: LogLevel::Error,
        message: "migrated message".to_string(),
    };

    fs::create_dir_all(temp_dir.join("workspaces")).expect("create workspaces dir");
    let legacy_workspace = serde_json::json!({
        "version": 1,
        "workspace_id": "workspace-2",
        "workspace_name": "Workspace 2",
        "hydroserver_url": "https://example.com",
        "datasources": [
            {
                "id": "job-1",
                "name": "Migrated source",
                "enabled": true,
                "file_path": "/tmp/migrated.csv",
                "schedule_minutes": 15,
                "file_config": {
                    "headerRow": 1,
                    "dataStartRow": 2,
                    "delimiter": ",",
                    "identifierType": "name",
                    "timestamp": {
                        "key": "timestamp",
                        "format": "ISO8601",
                        "timezoneMode": "embeddedOffset"
                    }
                },
                "column_mappings": [],
                "recent_logs": [
                    {
                        "timestamp": migrated_entry.timestamp,
                        "level": "error",
                        "message": "migrated message"
                    }
                ]
            }
        ]
    });
    fs::write(
        temp_dir.join("workspaces").join("workspace-2.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&legacy_workspace).expect("serialize workspace")
        ),
    )
    .expect("write workspace");

    let logs = store.logs_for("job-1", 200).expect("load migrated logs");
    assert_eq!(logs, vec![migrated_entry]);

    let workspace_contents =
        fs::read_to_string(temp_dir.join("workspaces").join("workspace-2.json"))
            .expect("read migrated workspace");
    assert!(
        !workspace_contents.contains("recent_logs"),
        "workspace JSON should be rewritten without embedded recent logs after migration"
    );
    assert!(
        store
            .job_log_file_path("job-1")
            .expect("get migrated log path")
            .is_some(),
        "migration should create a durable job log file"
    );

    remove_temp_dir(&temp_dir);
}

#[test]
fn loading_workspace_rewrites_generated_test_csv_paths_into_current_config_dir() {
    let temp_dir = unique_temp_dir("config-store-generated-paths");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");

    fs::write(
        temp_dir.join("config.json"),
        r#"{
  "version": 1,
  "server": {
    "auth_type": "apikey",
    "url": "https://example.com",
    "api_key": "",
    "username": "",
    "password": "",
    "workspace_id": "workspace-3",
    "workspace_name": "Workspace 3"
  }
}
"#,
    )
    .expect("write config");

    let generated_dir = temp_dir.join("generated-test-csv").join("sample-batch");
    fs::create_dir_all(&generated_dir).expect("create generated test dir");
    let csv_path = generated_dir.join("sample.csv");
    fs::write(&csv_path, "timestamp,value\n2026-04-15T00:00:00Z,1\n").expect("write csv");

    fs::create_dir_all(temp_dir.join("workspaces")).expect("create workspaces dir");
    let legacy_workspace = serde_json::json!({
        "version": 1,
        "workspace_id": "workspace-3",
        "workspace_name": "Workspace 3",
        "hydroserver_url": "https://example.com",
        "datasources": [
            {
                "id": "job-1",
                "name": "Generated source",
                "enabled": true,
                "file_path": "/Users/example/Library/Application Support/com.streaming-data-loader.app/generated-test-csv/sample-batch/sample.csv",
                "schedule_minutes": 15,
                "file_config": {
                    "headerRow": 1,
                    "dataStartRow": 2,
                    "delimiter": ",",
                    "identifierType": "name",
                    "timestamp": {
                        "key": "timestamp",
                        "format": "ISO8601",
                        "timezoneMode": "embeddedOffset"
                    }
                },
                "column_mappings": []
            }
        ]
    });
    fs::write(
        temp_dir.join("workspaces").join("workspace-3.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&legacy_workspace).expect("serialize workspace")
        ),
    )
    .expect("write workspace");

    let job = store
        .get_job("job-1")
        .expect("load job")
        .expect("job should exist");
    assert_eq!(job.file_path, csv_path.to_string_lossy());

    let workspace_contents =
        fs::read_to_string(temp_dir.join("workspaces").join("workspace-3.json"))
            .expect("read workspace");
    assert!(
        workspace_contents.contains(csv_path.to_string_lossy().as_ref()),
        "workspace JSON should be rewritten to the current generated-test-csv location"
    );

    remove_temp_dir(&temp_dir);
}

#[test]
fn running_state_is_persisted_and_can_be_cleared_globally() {
    let temp_dir = unique_temp_dir("config-store-running");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");
    store
        .set_server(
            ServerConfig {
                url: "https://example.com".to_string(),
                workspace_id: "workspace-running".to_string(),
                workspace_name: "Workspace Running".to_string(),
                ..ServerConfig::default()
            },
            "Workspace Running",
        )
        .expect("set server");

    let job = store
        .create_job(JobUpsertRequest {
            name: "Running source".to_string(),
            enabled: true,
            file_path: "/tmp/running.csv".to_string(),
            schedule_minutes: 15,
            file_config: FileConfig::default(),
            column_mappings: Vec::new(),
        })
        .expect("create job");

    store
        .set_job_running(&job.id, true)
        .expect("set job running");
    assert!(
        store.cursor_for(&job.id).expect("load cursor").is_running,
        "cursor should reflect persisted running state"
    );

    store
        .clear_all_running_jobs()
        .expect("clear all running jobs");
    assert!(
        !store.cursor_for(&job.id).expect("load cursor").is_running,
        "global running-state reset should clear persisted flags"
    );

    remove_temp_dir(&temp_dir);
}

/// A successful upload for one datastream should advance only that
/// datastream's cursor, leaving a behind sibling's cursor and error intact.
/// The job-level aggregate must then reflect the MIN of the two.
#[test]
fn record_datastream_success_isolates_per_datastream_state() {
    let temp_dir = unique_temp_dir("config-store-per-ds-success");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");
    store
        .set_server(
            ServerConfig {
                url: "https://example.com".to_string(),
                workspace_id: "workspace-partial".to_string(),
                workspace_name: "Partial Failure".to_string(),
                ..ServerConfig::default()
            },
            "Partial Failure",
        )
        .expect("set server");

    let job = store
        .create_job(JobUpsertRequest {
            name: "Multi-datastream job".to_string(),
            enabled: true,
            file_path: "/tmp/multi.csv".to_string(),
            schedule_minutes: 15,
            file_config: FileConfig::default(),
            column_mappings: vec![
                ColumnMapping {
                    csv_column: "Stage_ft".to_string(),
                    datastream_id: "ds-stage".to_string(),
                    datastream_name: "Stage".to_string(),
                },
                ColumnMapping {
                    csv_column: "WaterTemp_C".to_string(),
                    datastream_id: "ds-temp".to_string(),
                    datastream_name: "Water Temp".to_string(),
                },
            ],
        })
        .expect("create job");

    // Simulate a prior failure on ds-temp at row 5.
    let failed_at = Utc::now();
    store
        .record_datastream_failure(&job.id, "ds-temp", "network error", failed_at)
        .expect("record temp failure");

    // Now record a success on ds-stage all the way through row 8.
    let stage_ts = chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(8, 20, 0)
        .unwrap()
        .and_utc();
    store
        .record_datastream_success(&job.id, "ds-stage", 8, stage_ts, Utc::now())
        .expect("record stage success");

    let cursor = store.cursor_for(&job.id).expect("load cursor");

    let stage = cursor
        .datastream_cursors
        .get("ds-stage")
        .expect("stage cursor present");
    assert_eq!(stage.last_pushed_row_index, Some(8));
    assert_eq!(stage.last_error, None);

    let temp = cursor
        .datastream_cursors
        .get("ds-temp")
        .expect("temp cursor present");
    assert_eq!(
        temp.last_pushed_row_index, None,
        "failed sibling's cursor must not advance"
    );
    assert_eq!(
        temp.last_error.as_deref(),
        Some("network error"),
        "failed sibling's error must survive the other datastream's success"
    );

    // Job-level aggregate is MIN across mappings: ds-temp has no row yet, so
    // the aggregate should be None (can't skip rows any datastream still
    // needs).
    assert_eq!(
        cursor.last_pushed_row_index, None,
        "aggregate row must be None while any mapping has no confirmed cursor"
    );
    assert_eq!(
        cursor.last_error.as_deref(),
        Some("network error"),
        "aggregate error should surface the still-failing datastream"
    );

    remove_temp_dir(&temp_dir);
}

/// Fix #2: record_datastream_failure should only touch the specified
/// datastream's cursor, never the sibling's confirmed state.
#[test]
fn record_datastream_failure_preserves_sibling_progress() {
    let temp_dir = unique_temp_dir("config-store-per-ds-failure");
    let store = ConfigStore::new(temp_dir.clone());
    store.ensure().expect("ensure store");
    store
        .set_server(
            ServerConfig {
                url: "https://example.com".to_string(),
                workspace_id: "workspace-fail".to_string(),
                workspace_name: "Fail Isolation".to_string(),
                ..ServerConfig::default()
            },
            "Fail Isolation",
        )
        .expect("set server");

    let job = store
        .create_job(JobUpsertRequest {
            name: "Multi-datastream job".to_string(),
            enabled: true,
            file_path: "/tmp/multi.csv".to_string(),
            schedule_minutes: 15,
            file_config: FileConfig::default(),
            column_mappings: vec![
                ColumnMapping {
                    csv_column: "Stage_ft".to_string(),
                    datastream_id: "ds-stage".to_string(),
                    datastream_name: "Stage".to_string(),
                },
                ColumnMapping {
                    csv_column: "WaterTemp_C".to_string(),
                    datastream_id: "ds-temp".to_string(),
                    datastream_name: "Water Temp".to_string(),
                },
            ],
        })
        .expect("create job");

    let stage_ts = chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(8, 20, 0)
        .unwrap()
        .and_utc();
    let temp_ts = chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(8, 05, 0)
        .unwrap()
        .and_utc();
    store
        .record_datastream_success(&job.id, "ds-stage", 8, stage_ts, Utc::now())
        .expect("record stage success");
    store
        .record_datastream_success(&job.id, "ds-temp", 5, temp_ts, Utc::now())
        .expect("record temp success");

    // Now record a failure on ds-temp — stage's confirmed cursor at row 8
    // must not regress.
    store
        .record_datastream_failure(&job.id, "ds-temp", "timeout", Utc::now())
        .expect("record temp failure");

    let cursor = store.cursor_for(&job.id).expect("load cursor");

    let stage = cursor
        .datastream_cursors
        .get("ds-stage")
        .expect("stage cursor");
    assert_eq!(stage.last_pushed_row_index, Some(8));
    assert_eq!(stage.last_error, None);

    let temp = cursor
        .datastream_cursors
        .get("ds-temp")
        .expect("temp cursor");
    assert_eq!(
        temp.last_pushed_row_index,
        Some(5),
        "temp's prior successful cursor must persist through a later failure"
    );
    assert_eq!(temp.last_error.as_deref(), Some("timeout"));

    // Job-level aggregate row is MIN(5, 8) = 5 — matches what the scan needs
    // in order to backtrack for the still-failing datastream.
    assert_eq!(cursor.last_pushed_row_index, Some(5));

    remove_temp_dir(&temp_dir);
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
