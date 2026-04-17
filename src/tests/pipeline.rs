use super::{
    normalize_watched_path, overdue_paths, read_csv_rows, scan_job_file, PipelineService, ScanMode,
    WatchPlan,
};
use crate::{
    config_store::ConfigStore,
    hydroserver::HydroServerService,
    models::{
        AuthType, ColumnMapping, FileConfig, IdentifierType, JobConfig, JobCursor,
        JobUpsertRequest, ServerConfig, TimestampConfig,
    },
};
use chrono::Utc;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn sample_job(path: &str) -> JobConfig {
    JobConfig {
        id: "job-1".to_string(),
        name: "Example".to_string(),
        enabled: true,
        file_path: path.to_string(),
        schedule_minutes: 15,
        file_config: FileConfig {
            header_row: Some(3),
            data_start_row: 4,
            delimiter: ",".to_string(),
            identifier_type: IdentifierType::Name,
            timestamp: TimestampConfig::default(),
        },
        column_mappings: vec![ColumnMapping {
            csv_column: "Stage_ft".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Stage".to_string(),
        }],
    }
}

fn sample_job_request(path: &str) -> JobUpsertRequest {
    let sample = sample_job(path);
    JobUpsertRequest {
        name: sample.name,
        enabled: sample.enabled,
        file_path: sample.file_path,
        schedule_minutes: sample.schedule_minutes,
        file_config: sample.file_config,
        column_mappings: sample.column_mappings,
    }
}

fn sample_server(url: String) -> ServerConfig {
    ServerConfig {
        auth_type: AuthType::Apikey,
        url,
        api_key: "test-api-key".to_string(),
        workspace_id: "workspace-1".to_string(),
        workspace_name: "Test Workspace".to_string(),
        ..ServerConfig::default()
    }
}

fn temp_test_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sdl-{label}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn read_csv_rows_allows_variable_width_preamble_rows() {
    let csv_text = "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
";

    let rows = read_csv_rows(csv_text, ',').expect("csv should parse");

    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0], vec!["Station", "Example Creek at Demo Site"]);
    assert_eq!(rows[2].len(), 3);
}

#[test]
fn scan_job_file_only_returns_appended_rows() {
    let path = std::env::temp_dir().join(format!(
        "sdl-pipeline-test-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(
        &path,
        "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
    )
    .expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().expect("utf-8 path")),
        4,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("scan should succeed");

    assert_eq!(result.file_row_count, 5);
    assert_eq!(result.observations.len(), 1);
    assert_eq!(result.observations[0].row_index, 5);

    let _ = std::fs::remove_file(path);
}

#[test]
fn scan_persists_row_count_across_successive_events() {
    let path = std::env::temp_dir().join(format!(
        "sdl-pipeline-persist-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // Initial write: 3 header/preamble rows + 2 data rows = 5 total
    std::fs::write(
        &path,
        "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
    )
    .expect("write csv");

    let job = sample_job(path.to_str().expect("utf-8 path"));

    // First scan with previous_row_count=0 sees both data rows
    let result1 =
        scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental).expect("scan 1");
    assert_eq!(result1.file_row_count, 5);
    assert_eq!(result1.observations.len(), 2);

    // Second scan with previous_row_count=5 sees nothing new
    let result2 = scan_job_file(
        job.clone(),
        result1.file_row_count,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("scan 2");
    assert_eq!(result2.observations.len(), 0);

    // Append one row
    std::fs::write(
        &path,
        "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
",
    )
    .expect("append csv");

    // Third scan with previous_row_count=5 sees only the new row
    let result3 = scan_job_file(
        job.clone(),
        result2.file_row_count,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("scan 3");
    assert_eq!(result3.file_row_count, 6);
    assert_eq!(result3.observations.len(), 1);
    assert_eq!(result3.observations[0].row_index, 6);

    let _ = std::fs::remove_file(path);
}

#[test]
fn scan_detects_file_truncation_and_rescans() {
    let path = std::env::temp_dir().join(format!(
        "sdl-pipeline-truncate-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    std::fs::write(
        &path,
        "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 09:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
",
    )
    .expect("write csv");

    let job = sample_job(path.to_str().expect("utf-8 path"));

    // First scan sees 3 data rows
    let result1 =
        scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental).expect("scan 1");
    assert_eq!(result1.file_row_count, 6);
    assert_eq!(result1.observations.len(), 3);

    // Truncate and rewrite with fewer rows
    std::fs::write(
        &path,
        "\
Station,Example Creek at Demo Site
Generated At,2026-04-03 10:00:00
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 09:00:00,2.60,8.1
",
    )
    .expect("rewrite csv");

    // Scan detects reset (4 < 6) and rescans from data_start_row
    let result2 = scan_job_file(
        job.clone(),
        result1.file_row_count,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("scan 2");
    assert!(result2.reset_detected);
    assert_eq!(result2.file_row_count, 4);
    assert_eq!(result2.observations.len(), 1);

    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------
// Edge-case tests for real-world CSV files
// ---------------------------------------------------------------

/// Many environmental-monitoring loggers emit 50–200 lines of metadata
/// (station name, serial number, units row, etc.) before the actual
/// header + data begin.  The user sets `header_row` and `data_start_row`
/// to skip past all of that.
#[test]
fn large_comment_preamble_is_skipped_correctly() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-preamble-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let mut csv = String::new();
    // 100 lines of free-form metadata
    for i in 1..=100 {
        csv.push_str(&format!("Comment line {i}: some logger metadata\n"));
    }
    // header on row 101, data starts at row 102
    csv.push_str("Timestamp,Stage_ft,WaterTemp_C\n");
    csv.push_str("2026-04-03 08:00:00,2.41,7.8\n");
    csv.push_str("2026-04-03 08:05:00,2.45,7.9\n");
    csv.push_str("2026-04-03 08:10:00,2.50,8.0\n");

    std::fs::write(&path, &csv).expect("write csv");

    let job = JobConfig {
        id: "job-preamble".to_string(),
        name: "Preamble Test".to_string(),
        enabled: true,
        file_path: path.to_str().unwrap().to_string(),
        schedule_minutes: 15,
        file_config: FileConfig {
            header_row: Some(101),
            data_start_row: 102,
            delimiter: ",".to_string(),
            identifier_type: IdentifierType::Name,
            timestamp: TimestampConfig::default(),
        },
        column_mappings: vec![ColumnMapping {
            csv_column: "Stage_ft".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Stage".to_string(),
        }],
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("scan with large preamble");
    assert_eq!(result.file_row_count, 104);
    assert_eq!(result.observations.len(), 3);
    assert_eq!(result.observations[0].row_index, 102);
    assert_eq!(result.observations[2].row_index, 104);

    let _ = std::fs::remove_file(path);
}

/// Campbell Scientific CR1000-style files have a 4-line header: station
/// info, column names, units row, and processing description — only the
/// second line is the "real" header.
#[test]
fn campbell_scientific_style_four_line_header() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-campbell-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
\"TOA5\",\"CR1000\",\"CPU:TestSite.CR1X\",\"12345\",\"CR1000.Std.32.06\",\"30490\",\"MyTable\"
\"TIMESTAMP\",\"RECORD\",\"Stage_ft\",\"WaterTemp_C\"
\"TS\",\"RN\",\"ft\",\"Deg C\"
\"\",\"\",\"Avg\",\"Avg\"
\"2026-04-03 08:00:00\",1,2.41,7.8
\"2026-04-03 08:05:00\",2,2.45,7.9
\"2026-04-03 08:10:00\",3,2.50,8.0
";

    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        id: "job-campbell".to_string(),
        name: "Campbell".to_string(),
        enabled: true,
        file_path: path.to_str().unwrap().to_string(),
        schedule_minutes: 15,
        file_config: FileConfig {
            header_row: Some(2),
            data_start_row: 5,
            delimiter: ",".to_string(),
            identifier_type: IdentifierType::Name,
            timestamp: TimestampConfig {
                key: "TIMESTAMP".to_string(),
                ..TimestampConfig::default()
            },
        },
        column_mappings: vec![ColumnMapping {
            csv_column: "Stage_ft".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Stage".to_string(),
        }],
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("scan campbell file");
    assert_eq!(result.observations.len(), 3);
    assert_eq!(result.observations[0].row_index, 5);

    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_csv_file_returns_zero_observations() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-empty-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    std::fs::write(&path, "").expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("empty file should not error");
    assert_eq!(result.file_row_count, 0);
    assert_eq!(result.observations.len(), 0);
    assert!(!result.reset_detected);

    let _ = std::fs::remove_file(path);
}

#[test]
fn header_only_file_returns_zero_observations() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-headeronly-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    std::fs::write(
        &path,
        "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
",
    )
    .expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("header-only should succeed");
    assert_eq!(result.file_row_count, 3);
    assert_eq!(result.observations.len(), 0);

    let _ = std::fs::remove_file(path);
}

/// Real sensor data often has gaps — e.g. a data column is blank when
/// the sensor was offline.  Blank observation values should be skipped
/// without breaking other columns or rows.
#[test]
fn sparse_rows_with_missing_values_are_handled() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-sparse-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,,7.9
2026-04-03 08:10:00,2.50,
2026-04-03 08:15:00,,
2026-04-03 08:20:00,2.55,8.1
";

    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        column_mappings: vec![
            ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-stage".to_string(),
                datastream_name: "Stage".to_string(),
            },
            ColumnMapping {
                csv_column: "WaterTemp_C".to_string(),
                datastream_id: "ds-temp".to_string(),
                datastream_name: "Temp".to_string(),
            },
        ],
        ..sample_job(path.to_str().unwrap())
    };

    let result =
        scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental).expect("sparse scan");

    // Row 4: both present (2 obs), row 5: temp only (1), row 6: stage only (1),
    // row 7: neither (0), row 8: both (2) => 6 total
    assert_eq!(result.observations.len(), 6);

    // Verify the observations are from the right datastreams
    let stage_obs: Vec<_> = result
        .observations
        .iter()
        .filter(|o| o.datastream_id == "ds-stage")
        .collect();
    let temp_obs: Vec<_> = result
        .observations
        .iter()
        .filter(|o| o.datastream_id == "ds-temp")
        .collect();
    assert_eq!(stage_obs.len(), 3); // rows 4, 6, 8
    assert_eq!(temp_obs.len(), 3); // rows 4, 5, 8

    let _ = std::fs::remove_file(path);
}

/// Empty timestamp rows should be silently skipped.
#[test]
fn rows_with_empty_timestamps_are_skipped() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-emptyts-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
   ,2.55,8.1
2026-04-03 08:20:00,2.60,8.2
";

    std::fs::write(&path, csv).expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("empty-ts scan");

    // Only rows with valid timestamps: 4, 6, 8
    assert_eq!(result.observations.len(), 3);
    assert_eq!(result.observations[0].row_index, 4);
    assert_eq!(result.observations[1].row_index, 6);
    assert_eq!(result.observations[2].row_index, 8);

    let _ = std::fs::remove_file(path);
}

/// Quoted CSV fields containing the delimiter character itself.
#[test]
fn quoted_fields_with_embedded_commas() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-quoted-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // The preamble has commas inside quotes; data values should still parse.
    let csv = "\
\"Station Name\",\"Example Creek, East Fork\"
\"Generated At\",\"April 3, 2026\"
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";

    std::fs::write(&path, csv).expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("quoted-fields scan");
    assert_eq!(result.observations.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// Tab-delimited files are common from certain loggers and spreadsheet
/// exports.
#[test]
fn tab_delimited_file() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-tab-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "Station\tExample Creek\n\
                Generated At\t2026-04-03\n\
                Timestamp\tStage_ft\tWaterTemp_C\n\
                2026-04-03 08:00:00\t2.41\t7.8\n\
                2026-04-03 08:05:00\t2.45\t7.9\n";

    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        file_config: FileConfig {
            delimiter: "\t".to_string(),
            ..sample_job("").file_config.clone()
        },
        ..sample_job(path.to_str().unwrap())
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("tab-delimited scan");
    assert_eq!(result.observations.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// Windows tools write \r\n line endings.  The csv crate strips them,
/// but we should verify the pipeline handles this end-to-end.
#[test]
fn windows_crlf_line_endings() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-crlf-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "Station,Example Creek\r\n\
                Generated At,2026-04-03\r\n\
                Timestamp,Stage_ft,WaterTemp_C\r\n\
                2026-04-03 08:00:00,2.41,7.8\r\n\
                2026-04-03 08:05:00,2.45,7.9\r\n";

    std::fs::write(&path, csv).expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("crlf scan");
    assert_eq!(result.observations.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// Some users configure jobs by column index rather than name
/// (e.g. when there is no header row, or it's unreliable).
#[test]
fn index_based_column_identification() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-index-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // No meaningful header — data starts immediately at row 1
    let csv = "\
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
";
    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        id: "job-index".to_string(),
        name: "Index Job".to_string(),
        enabled: true,
        file_path: path.to_str().unwrap().to_string(),
        schedule_minutes: 15,
        file_config: FileConfig {
            header_row: None,
            data_start_row: 1,
            delimiter: ",".to_string(),
            identifier_type: IdentifierType::Index,
            timestamp: TimestampConfig {
                key: "1".to_string(), // column 1 = timestamp
                ..TimestampConfig::default()
            },
        },
        column_mappings: vec![ColumnMapping {
            csv_column: "2".to_string(), // column 2 = Stage
            datastream_id: "ds-1".to_string(),
            datastream_name: "Stage".to_string(),
        }],
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("index-based scan");
    assert_eq!(result.observations.len(), 3);

    let _ = std::fs::remove_file(path);
}

/// Column name lookup should be case-insensitive ("timestamp" matches
/// "TIMESTAMP" or "Timestamp").
#[test]
fn case_insensitive_header_matching() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-case-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
TIMESTAMP,STAGE_FT,WATERTEMP_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";
    std::fs::write(&path, csv).expect("write csv");

    // Job config uses lowercase column names vs uppercase in file
    let job = JobConfig {
        file_config: FileConfig {
            timestamp: TimestampConfig {
                key: "timestamp".to_string(),
                ..TimestampConfig::default()
            },
            ..sample_job("").file_config.clone()
        },
        column_mappings: vec![ColumnMapping {
            csv_column: "stage_ft".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Stage".to_string(),
        }],
        ..sample_job(path.to_str().unwrap())
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("case-insensitive scan");
    assert_eq!(result.observations.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// FullResync with a cursor should skip rows that were already pushed,
/// so a "Run Now" doesn't re-upload the full history.
#[test]
fn full_resync_respects_cursor() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-resync-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
2026-04-03 08:15:00,2.55,8.1
";
    std::fs::write(&path, csv).expect("write csv");

    // Cursor says we already pushed through 08:05 (row 5)
    let cursor = JobCursor {
        last_pushed_timestamp: Some(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
                .unwrap()
                .and_hms_opt(8, 5, 0)
                .unwrap()
                .and_utc(),
        ),
        last_pushed_row_index: Some(5),
        last_run_at: None,
        last_error: None,
        is_running: false,
    };

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        cursor,
        ScanMode::FullResync,
    )
    .expect("full resync scan");

    // Should only return rows after the cursor: 08:10 and 08:15
    assert_eq!(result.observations.len(), 2);
    assert_eq!(result.observations[0].row_index, 6);
    assert_eq!(result.observations[1].row_index, 7);

    let _ = std::fs::remove_file(path);
}

/// Multiple column mappings from the same file — each data row should
/// produce one observation per mapping (when the value is non-empty).
#[test]
fn multiple_column_mappings_produce_correct_observations() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-multi-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C,Conductivity
2026-04-03 08:00:00,2.41,7.8,145.2
2026-04-03 08:05:00,2.45,7.9,146.0
";
    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        column_mappings: vec![
            ColumnMapping {
                csv_column: "Stage_ft".to_string(),
                datastream_id: "ds-stage".to_string(),
                datastream_name: "Stage".to_string(),
            },
            ColumnMapping {
                csv_column: "WaterTemp_C".to_string(),
                datastream_id: "ds-temp".to_string(),
                datastream_name: "Temp".to_string(),
            },
            ColumnMapping {
                csv_column: "Conductivity".to_string(),
                datastream_id: "ds-cond".to_string(),
                datastream_name: "Cond".to_string(),
            },
        ],
        ..sample_job(path.to_str().unwrap())
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental)
        .expect("multi-mapping scan");

    // 2 data rows * 3 mappings = 6 observations
    assert_eq!(result.observations.len(), 6);

    let stage: Vec<_> = result
        .observations
        .iter()
        .filter(|o| o.datastream_id == "ds-stage")
        .collect();
    let temp: Vec<_> = result
        .observations
        .iter()
        .filter(|o| o.datastream_id == "ds-temp")
        .collect();
    let cond: Vec<_> = result
        .observations
        .iter()
        .filter(|o| o.datastream_id == "ds-cond")
        .collect();
    assert_eq!(stage.len(), 2);
    assert_eq!(temp.len(), 2);
    assert_eq!(cond.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// Values with leading/trailing whitespace should be trimmed and still
/// parse correctly as numbers.
#[test]
fn whitespace_padded_values_are_trimmed() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-ws-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
  2026-04-03 08:00:00  ,  2.41  ,  7.8
  2026-04-03 08:05:00  ,  2.45  ,  7.9
";
    std::fs::write(&path, csv).expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("whitespace scan");
    assert_eq!(result.observations.len(), 2);

    // Values should parse as floats, not strings
    assert_eq!(result.observations[0].value, serde_json::json!(2.41));
    assert_eq!(result.observations[1].value, serde_json::json!(2.45));

    let _ = std::fs::remove_file(path);
}

/// File that doesn't exist should return a clear error, not a panic.
#[test]
fn missing_file_produces_clear_error() {
    let path = "/tmp/sdl-nonexistent-file-that-does-not-exist-99999.csv";
    let result = scan_job_file(
        sample_job(path),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("No such file") || msg.contains("not found") || msg.contains("cannot find"),
        "error should mention the missing file: {msg}"
    );
}

/// A column referenced in the mapping that doesn't exist in the header
/// should produce a clear error pointing at the column name.
#[test]
fn missing_column_produces_clear_error() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-missingcol-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
";
    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        column_mappings: vec![ColumnMapping {
            csv_column: "Discharge_cfs".to_string(), // does not exist
            datastream_id: "ds-1".to_string(),
            datastream_name: "Discharge".to_string(),
        }],
        ..sample_job(path.to_str().unwrap())
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("Discharge_cfs"),
        "error should name the missing column: {msg}"
    );

    let _ = std::fs::remove_file(path);
}

/// Values that look like strings (e.g. "good", "suspect") should be
/// preserved as JSON strings, while numbers become JSON numbers.
#[test]
fn mixed_numeric_and_string_observation_values() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-mixed-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,low,sensor_error
2026-04-03 08:10:00,-0.5,0
";
    std::fs::write(&path, csv).expect("write csv");

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        0,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("mixed values scan");
    assert_eq!(result.observations.len(), 3);

    assert_eq!(result.observations[0].value, serde_json::json!(2.41));
    assert_eq!(result.observations[1].value, serde_json::json!("low"));
    assert_eq!(result.observations[2].value, serde_json::json!(-0.5));

    let _ = std::fs::remove_file(path);
}

/// Semicolon-delimited files (common in European locales).
#[test]
fn semicolon_delimited_file() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-semi-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station;Example Creek\n\
Generated At;2026-04-03\n\
Timestamp;Stage_ft;WaterTemp_C\n\
2026-04-03 08:00:00;2.41;7.8\n\
2026-04-03 08:05:00;2.45;7.9\n";

    std::fs::write(&path, csv).expect("write csv");

    let job = JobConfig {
        file_config: FileConfig {
            delimiter: ";".to_string(),
            ..sample_job("").file_config.clone()
        },
        ..sample_job(path.to_str().unwrap())
    };

    let result =
        scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental).expect("semicolon scan");
    assert_eq!(result.observations.len(), 2);

    let _ = std::fs::remove_file(path);
}

/// Incremental scan where previous_row_count is beyond the
/// data_start_row but the file hasn't grown — zero observations.
#[test]
fn incremental_no_change_returns_zero() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-nochange-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
";
    std::fs::write(&path, csv).expect("write csv");

    // Simulate having already seen all 5 rows
    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        5,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("no-change scan");
    assert_eq!(result.observations.len(), 0);
    assert!(!result.reset_detected);

    let _ = std::fs::remove_file(path);
}

/// BOM-prefixed UTF-8 files (common when CSV is saved from Excel).
/// The BOM bytes (\xEF\xBB\xBF) must not corrupt the first field.
#[test]
fn utf8_bom_does_not_corrupt_first_column() {
    let path = std::env::temp_dir().join(format!(
        "sdl-edge-bom-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // Write BOM + CSV content
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"Timestamp,Stage_ft,WaterTemp_C\n");
    bytes.extend_from_slice(b"2026-04-03 08:00:00,2.41,7.8\n");
    bytes.extend_from_slice(b"2026-04-03 08:05:00,2.45,7.9\n");
    std::fs::write(&path, &bytes).expect("write bom csv");

    let job = JobConfig {
        id: "job-bom".to_string(),
        name: "BOM Test".to_string(),
        enabled: true,
        file_path: path.to_str().unwrap().to_string(),
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
    };

    let result = scan_job_file(job, 0, JobCursor::default(), ScanMode::Incremental);

    // The BOM bytes become part of the first cell when read as UTF-8.
    // This is a known limitation — verify the test captures current behavior.
    // If this fails with a column-not-found error, we need a BOM-stripping fix.
    match &result {
        Ok(r) => {
            // If it works, great — both observations should be present
            assert_eq!(r.observations.len(), 2);
        }
        Err(msg) => {
            // If it fails because the BOM corrupted "Timestamp", that's a
            // real bug we need to fix.
            assert!(
                msg.contains("Timestamp") || msg.contains("not found"),
                "unexpected error: {msg}"
            );
            // Flag this as a known issue rather than letting it silently pass
            panic!("BOM corrupts the first header cell — needs a fix in scan_job_file: {msg}");
        }
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn scheduler_overdue_path_selection_uses_shortest_job_interval_and_skips_unscanned_paths() {
    let now = Instant::now();
    let shared_path = PathBuf::from("/tmp/sdl-scheduler-shared.csv");
    let fifteen_minute_path = PathBuf::from("/tmp/sdl-scheduler-fifteen.csv");
    let never_scanned_path = PathBuf::from("/tmp/sdl-scheduler-never.csv");

    let watch_plan = WatchPlan {
        jobs_by_path: HashMap::from([
            (
                shared_path.clone(),
                vec![
                    JobConfig {
                        id: "job-fast".to_string(),
                        schedule_minutes: 5,
                        ..sample_job(shared_path.to_str().expect("utf-8 path"))
                    },
                    JobConfig {
                        id: "job-slow".to_string(),
                        schedule_minutes: 30,
                        ..sample_job(shared_path.to_str().expect("utf-8 path"))
                    },
                ],
            ),
            (
                fifteen_minute_path.clone(),
                vec![sample_job(
                    fifteen_minute_path.to_str().expect("utf-8 path"),
                )],
            ),
            (
                never_scanned_path.clone(),
                vec![JobConfig {
                    id: "job-never".to_string(),
                    schedule_minutes: 1,
                    ..sample_job(never_scanned_path.to_str().expect("utf-8 path"))
                }],
            ),
        ]),
        server: None,
    };

    let last_scan_times = HashMap::from([
        (shared_path.clone(), now - Duration::from_secs(6 * 60)),
        (
            fifteen_minute_path.clone(),
            now - Duration::from_secs(14 * 60),
        ),
    ]);

    let overdue: HashSet<_> = overdue_paths(now, &watch_plan, &last_scan_times)
        .into_iter()
        .collect();

    assert!(
        overdue.contains(&shared_path),
        "a shared path should use the shortest schedule interval across its jobs"
    );
    assert!(
        !overdue.contains(&fifteen_minute_path),
        "a 15-minute job scanned 14 minutes ago should not be overdue yet"
    );
    assert!(
        !overdue.contains(&never_scanned_path),
        "paths with no recorded scan time should wait for the initial scan queued by reload"
    );
}

#[test]
fn large_csv_scan_produces_bounded_observations() {
    let path = std::env::temp_dir().join(format!(
        "sdl-pipeline-large-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // Generate a 10,000-row CSV using epoch seconds to avoid invalid dates
    let mut csv = String::from(
        "Station,Example Creek\nGenerated At,2026-04-03\nTimestamp,Stage_ft,WaterTemp_C\n",
    );
    let base = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    for i in 0..10_000u64 {
        let ts = base + chrono::Duration::minutes(i as i64 * 5);
        csv.push_str(&format!(
            "{},{:.2},{:.1}\n",
            ts.format("%Y-%m-%dT%H:%M:%S"),
            2.0 + (i as f64) * 0.01,
            7.0 + (i as f64) * 0.001,
        ));
    }
    std::fs::write(&path, &csv).expect("write large csv");

    let job = sample_job(path.to_str().expect("utf-8 path"));

    // Full scan from row 0 — should produce exactly 10,000 observations
    let result = scan_job_file(job.clone(), 0, JobCursor::default(), ScanMode::Incremental)
        .expect("scan large file");
    assert_eq!(result.file_row_count, 10_003); // 3 header + 10,000 data
    assert_eq!(result.observations.len(), 10_000);

    // Incremental scan with previous_row_count = full file — should produce 0
    let result2 = scan_job_file(
        job.clone(),
        result.file_row_count,
        JobCursor::default(),
        ScanMode::Incremental,
    )
    .expect("incremental scan of unchanged file");
    assert_eq!(result2.observations.len(), 0);

    let _ = std::fs::remove_file(path);
}

/// Fix #1: On restart row_counts is empty (0).  load_cursor_row_seeds seeds it
/// from the persisted cursor so we don't re-scan already-uploaded rows.
/// This test verifies that scanning with previous_row_count seeded from the
/// cursor's last_pushed_row_index correctly skips already-pushed rows.
#[test]
fn scan_seeded_from_cursor_skips_already_pushed_rows() {
    let path = std::env::temp_dir().join(format!(
        "sdl-seed-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // 3 header rows + 4 data rows (rows 4-7 in 1-indexed terms)
    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
2026-04-03 08:15:00,2.55,8.1
";
    std::fs::write(&path, csv).expect("write csv");

    // Simulate: cursor says rows 4-6 were already pushed (max_row_index = 6).
    // Seed previous_row_count = 6 (as load_cursor_row_seeds would produce).
    // The scan should only return row 7.
    let cursor = JobCursor {
        last_pushed_timestamp: Some(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
                .unwrap()
                .and_hms_opt(8, 10, 0)
                .unwrap()
                .and_utc(),
        ),
        last_pushed_row_index: Some(6),
        last_run_at: None,
        last_error: None,
        is_running: false,
    };

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        6, // seeded from cursor.last_pushed_row_index
        cursor,
        ScanMode::Incremental,
    )
    .expect("seeded scan");

    assert_eq!(
        result.observations.len(),
        1,
        "only the new row should be returned"
    );
    assert_eq!(result.observations[0].row_index, 7);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn restart_load_cursor_row_seeds_prevents_duplicate_scans() {
    let temp_dir = temp_test_dir("restart-seeds");
    let config_dir = temp_dir.join("config");
    let csv_path = temp_dir.join("source.csv");

    std::fs::write(
        &csv_path,
        "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
    )
    .expect("write csv");

    let config_store = Arc::new(ConfigStore::new(config_dir));
    config_store.ensure().expect("ensure config store");
    config_store
        .set_server(
            sample_server("http://127.0.0.1:9".to_string()),
            "Test Workspace",
        )
        .expect("set server");
    let job = config_store
        .create_job(sample_job_request(csv_path.to_str().expect("utf-8 path")))
        .expect("create job");

    let first_runtime = PipelineService::new(
        config_store.clone(),
        Arc::new(HydroServerService::new().expect("hydroserver service")),
    );
    let first_snapshot = first_runtime
        .load_watch_plan()
        .await
        .expect("load watch plan");
    let normalized_path = normalize_watched_path(&csv_path);
    assert!(
        first_runtime
            .load_cursor_row_seeds(&first_snapshot)
            .await
            .get(&normalized_path)
            .is_none(),
        "new jobs should not have a row-count seed before any uploads succeed"
    );

    config_store
        .update_cursor(
            &job.id,
            JobCursor {
                last_pushed_timestamp: Some(
                    chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
                        .unwrap()
                        .and_hms_opt(8, 5, 0)
                        .unwrap()
                        .and_utc(),
                ),
                last_pushed_row_index: Some(5),
                last_run_at: Some(Utc::now()),
                last_error: None,
                is_running: false,
            },
        )
        .expect("persist cursor");

    std::fs::write(
        &csv_path,
        "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
",
    )
    .expect("append csv row");

    let restarted_runtime = PipelineService::new(
        config_store.clone(),
        Arc::new(HydroServerService::new().expect("hydroserver service")),
    );
    let restarted_snapshot = restarted_runtime
        .load_watch_plan()
        .await
        .expect("load watch plan after restart");
    let seeds = restarted_runtime
        .load_cursor_row_seeds(&restarted_snapshot)
        .await;
    assert_eq!(
        seeds.get(&normalized_path),
        Some(&5usize),
        "restart should seed row counts from the persisted cursor"
    );

    let cursor = config_store.cursor_for(&job.id).expect("load cursor");
    let result = scan_job_file(job, 5, cursor, ScanMode::Incremental).expect("scan after restart");
    assert_eq!(result.observations.len(), 1);
    assert_eq!(result.observations[0].row_index, 6);

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[tokio::test]
async fn shared_file_scans_use_one_baseline_for_all_jobs() {
    let temp_dir = temp_test_dir("shared-file-jobs");
    let config_dir = temp_dir.join("config");
    let csv_path = temp_dir.join("source.csv");

    std::fs::write(
        &csv_path,
        "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
",
    )
    .expect("write csv");

    let config_store = Arc::new(ConfigStore::new(config_dir));
    config_store.ensure().expect("ensure config store");
    let server = sample_server("http://127.0.0.1:9".to_string());
    config_store
        .set_server(server.clone(), "Test Workspace")
        .expect("set server");

    let first_job = config_store
        .create_job(sample_job_request(csv_path.to_str().expect("utf-8 path")))
        .expect("create first job");

    let mut second_request = sample_job_request(csv_path.to_str().expect("utf-8 path"));
    second_request.name = "Second job".to_string();
    second_request.column_mappings = vec![ColumnMapping {
        csv_column: "WaterTemp_C".to_string(),
        datastream_id: "ds-2".to_string(),
        datastream_name: "WaterTemp".to_string(),
    }];
    let second_job = config_store
        .create_job(second_request)
        .expect("create second job");

    let runtime = PipelineService::new(
        config_store,
        Arc::new(HydroServerService::new().expect("hydroserver service")),
    );
    let normalized_path = normalize_watched_path(&csv_path);

    runtime
        .inner
        .row_counts
        .lock()
        .await
        .insert(normalized_path.clone(), 4);

    let first_result = runtime
        .scan_job(
            normalized_path.clone(),
            Arc::new(server.clone()),
            first_job,
            4,
            ScanMode::Incremental,
        )
        .await
        .expect("scan first job");
    assert_eq!(first_result, 5);

    let second_result = runtime
        .scan_job(
            normalized_path.clone(),
            Arc::new(server),
            second_job,
            4,
            ScanMode::Incremental,
        )
        .await
        .expect("scan second job");
    assert_eq!(second_result, 5);

    let shared_row_count = runtime
        .inner
        .row_counts
        .lock()
        .await
        .get(&normalized_path)
        .copied();
    assert_eq!(
        shared_row_count,
        Some(4),
        "per-job scans should not advance the shared file baseline until the whole path scan completes"
    );

    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Fix #2: When the previous upload failed (cursor.last_error is set), the
/// scan must backtrack to cursor.last_pushed_row_index and retry the failed
/// rows, even if previous_row_count (in-memory) is already past them.
#[test]
fn scan_retries_failed_rows_when_cursor_has_error() {
    let path = std::env::temp_dir().join(format!(
        "sdl-retry-{}-{}.csv",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    // 3 header rows + 5 data rows (rows 4-8 in 1-indexed terms)
    let csv = "\
Station,Example Creek
Generated At,2026-04-03
Timestamp,Stage_ft,WaterTemp_C
2026-04-03 08:00:00,2.41,7.8
2026-04-03 08:05:00,2.45,7.9
2026-04-03 08:10:00,2.50,8.0
2026-04-03 08:15:00,2.55,8.1
2026-04-03 08:20:00,2.60,8.2
";
    std::fs::write(&path, csv).expect("write csv");

    // Scenario: rows 4-5 were pushed successfully (last_pushed_row_index=5).
    // Rows 6-8 were scanned and queued but the upload failed (last_error set).
    // In-memory previous_row_count = 8 (scan advanced past the failed rows).
    // Expected: incremental scan should backtrack to row 5 and re-queue rows 6-8.
    let cursor = JobCursor {
        last_pushed_timestamp: Some(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
                .unwrap()
                .and_hms_opt(8, 5, 0)
                .unwrap()
                .and_utc(),
        ),
        last_pushed_row_index: Some(5),
        last_run_at: None,
        last_error: Some("network error".to_string()),
        is_running: false,
    };

    let result = scan_job_file(
        sample_job(path.to_str().unwrap()),
        8, // in-memory row count is already at 8
        cursor,
        ScanMode::Incremental,
    )
    .expect("retry scan");

    // With Fix #2: should re-scan rows 6, 7, 8 (backtracked to last_pushed_row_index=5)
    assert_eq!(
        result.observations.len(),
        3,
        "failed rows should be retried"
    );
    assert_eq!(result.observations[0].row_index, 6);
    assert_eq!(result.observations[1].row_index, 7);
    assert_eq!(result.observations[2].row_index, 8);

    let _ = std::fs::remove_file(path);
}
