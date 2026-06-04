use super::*;
use crate::{
    config_store::ConfigStore,
    models::{
        AuthType, ColumnMapping, FileConfig, IdentifierType, JobUpsertRequest, LogLevel,
        ServerConfig, TimestampConfig,
    },
    observation_queue::bounded,
};
use serde_json::json;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{oneshot, Mutex},
    task::JoinHandle,
    time::timeout,
};

struct TestObservationServer {
    base_url: String,
    request_count: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<String>>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl TestObservationServer {
    async fn spawn(statuses: Vec<u16>) -> Self {
        Self::spawn_with_get_body(statuses, "{}".to_string()).await
    }

    /// Spawn a mock that pops `statuses` once per request. Successful responses
    /// to GET requests return `get_body` (used to serve `phenomenonEndTime` for
    /// conflict-reconciliation tests); successful POSTs return `{}`.
    async fn spawn_with_get_body(statuses: Vec<u16>, get_body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let request_count = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let statuses = Arc::new(Mutex::new(VecDeque::from(statuses)));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let task = tokio::spawn({
            let request_count = request_count.clone();
            let bodies = bodies.clone();
            let statuses = statuses.clone();

            async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        accept_result = listener.accept() => {
                            let Ok((mut socket, _)) = accept_result else {
                                break;
                            };
                            let Some((method, body)) = read_request(&mut socket).await else {
                                continue;
                            };

                            request_count.fetch_add(1, Ordering::SeqCst);
                            bodies.lock().await.push(body);

                            let status = statuses.lock().await.pop_front().unwrap_or(200);
                            let payload = if status >= 400 {
                                json!({ "detail": "temporary outage" }).to_string()
                            } else if method == "GET" {
                                get_body.clone()
                            } else {
                                "{}".to_string()
                            };
                            let response = format!(
                                "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                reason_phrase(status),
                                payload.len(),
                                payload
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        }
                    }
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            request_count,
            bodies,
            shutdown: Some(shutdown_tx),
            task,
        }
    }

    fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }

    async fn bodies(&self) -> Vec<String> {
        self.bodies.lock().await.clone()
    }

    async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

/// Read a request and return its method and body.
async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<(String, String)> {
    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut content_length = 0usize;
    let mut method = String::new();

    loop {
        let mut chunk = [0u8; 2048];
        let bytes_read = socket.read(&mut chunk).await.ok()?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);

        if header_end.is_none() {
            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                header_end = Some(index + 4);
                let headers = String::from_utf8_lossy(&buffer[..index + 4]);
                method = headers
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().next())
                    .unwrap_or_default()
                    .to_string();
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
            }
        }

        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                let body = String::from_utf8_lossy(&buffer[end..end + content_length]).into_owned();
                return Some((method, body));
            }
        }
    }

    header_end.map(|end| (method, String::from_utf8_lossy(&buffer[end..]).into_owned()))
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        409 => "Conflict",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "OK",
    }
}

fn temp_test_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "sdl-uploader-{label}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
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

fn sample_job_request(file_path: &str) -> JobUpsertRequest {
    JobUpsertRequest {
        name: "Uploader Test".to_string(),
        enabled: true,
        file_path: file_path.to_string(),
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

fn test_observation(job_id: &str, datastream_id: &str, row_index: u64) -> QueuedObservation {
    QueuedObservation {
        context: Arc::new(ObservationContext {
            server: Arc::new(crate::models::ServerConfig::default()),
            job_id: job_id.to_string(),
            datastream_id: datastream_id.to_string(),
            datastream_name: "Test".to_string(),
        }),
        timestamp: Utc::now(),
        row_index,
        value: json!(1.0),
    }
}

#[test]
fn batch_key_groups_by_server_and_datastream() {
    let obs_a1 = test_observation("job-1", "ds-a", 1);
    let obs_a2 = test_observation("job-1", "ds-a", 2);
    let obs_b1 = test_observation("job-1", "ds-b", 1);

    let key_a1 = BatchKey::from(&obs_a1);
    let key_a2 = BatchKey::from(&obs_a2);
    let key_b1 = BatchKey::from(&obs_b1);

    assert_eq!(key_a1, key_a2, "same datastream should produce same key");
    assert_ne!(key_a1, key_b1, "different datastreams should differ");
}

#[test]
fn summarize_batch_tracks_max_timestamp_and_row_index() {
    let t1 = Utc::now() - chrono::Duration::minutes(10);
    let t2 = Utc::now();

    let batch = PendingBatch {
        context: Arc::new(ObservationContext {
            server: Arc::new(crate::models::ServerConfig::default()),
            job_id: "job-1".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Test".to_string(),
        }),
        rows: vec![
            QueuedObservation {
                context: Arc::new(ObservationContext {
                    server: Arc::new(crate::models::ServerConfig::default()),
                    job_id: "job-1".to_string(),
                    datastream_id: "ds-1".to_string(),
                    datastream_name: "Test".to_string(),
                }),
                timestamp: t1,
                row_index: 5,
                value: json!(1.0),
            },
            QueuedObservation {
                context: Arc::new(ObservationContext {
                    server: Arc::new(crate::models::ServerConfig::default()),
                    job_id: "job-1".to_string(),
                    datastream_id: "ds-1".to_string(),
                    datastream_name: "Test".to_string(),
                }),
                timestamp: t2,
                row_index: 10,
                value: json!(2.0),
            },
        ],
    };

    let rows: Vec<&QueuedObservation> = batch.rows.iter().collect();
    let summaries = summarize_rows(&rows);
    let summary = summaries.get("job-1").expect("should have job-1");
    assert_eq!(summary.observation_count, 2);
    assert_eq!(summary.max_timestamp, t2);
    assert_eq!(summary.max_row_index, 10);
}

#[test]
fn batch_size_constant_is_reasonable() {
    assert_eq!(BATCH_SIZE, 500);
    assert_eq!(FLUSH_INTERVAL, Duration::from_secs(1));
}

#[tokio::test]
async fn upload_worker_retries_transient_server_errors_and_persists_success() {
    let server = TestObservationServer::spawn(vec![500, 200]).await;
    let temp_dir = temp_test_dir("retry");
    let config_dir = temp_dir.join("config");
    let source_path = temp_dir.join("source.csv");
    std::fs::write(&source_path, "placeholder").expect("write source placeholder");

    let config_store = Arc::new(ConfigStore::new(config_dir));
    config_store.ensure().expect("ensure config store");
    let server_config = sample_server(server.base_url.clone());
    config_store
        .set_server(server_config.clone(), "Test Workspace")
        .expect("set server");
    let job = config_store
        .create_job(sample_job_request(
            source_path.to_str().expect("utf-8 path"),
        ))
        .expect("create job");

    let (tx, rx) = bounded(8);
    let worker = spawn_upload_worker(
        rx,
        Arc::new(HydroServerService::new().expect("hydroserver service")),
        config_store.clone(),
    );

    let first_timestamp = chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(8, 0, 0)
        .unwrap()
        .and_utc();
    let second_timestamp = chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(8, 5, 0)
        .unwrap()
        .and_utc();

    let context = Arc::new(ObservationContext {
        server: Arc::new(server_config),
        job_id: job.id.clone(),
        datastream_id: "ds-1".to_string(),
        datastream_name: "Stage".to_string(),
    });

    tx.send(QueuedObservation {
        context: context.clone(),
        timestamp: first_timestamp,
        row_index: 4,
        value: json!(2.41),
    })
    .await
    .expect("send first observation");
    tx.send(QueuedObservation {
        context,
        timestamp: second_timestamp,
        row_index: 5,
        value: json!(2.45),
    })
    .await
    .expect("send second observation");
    drop(tx);

    timeout(Duration::from_secs(10), worker)
        .await
        .expect("worker timeout")
        .expect("join worker");

    assert_eq!(
        server.request_count(),
        2,
        "the uploader should retry once after the transient 500 response"
    );

    let bodies = server.bodies().await;
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        bodies[0], bodies[1],
        "retries should resend the same payload"
    );
    assert!(
        bodies[0].contains("\"phenomenonTime\"") && bodies[0].contains("\"result\""),
        "the request body should use the HydroServer bulk observation schema"
    );

    let cursor = config_store.cursor_for(&job.id).expect("load cursor");
    assert_eq!(cursor.last_error, None);
    assert_eq!(cursor.last_pushed_row_index, Some(5));
    assert_eq!(cursor.last_pushed_timestamp, Some(second_timestamp));

    let logs = config_store.logs_for(&job.id, 50).expect("load logs");
    assert!(
        logs.iter().any(|entry| {
            entry.level == LogLevel::Info
                && entry
                    .message
                    .contains("Loaded 2 observation(s) to datastream Stage.")
        }),
        "successful uploads should be recorded in the job log"
    );

    server.shutdown().await;
    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Shared setup for the conflict-reconciliation tests: a config store with a
/// configured server and one job. Returns the store, the created job, a server
/// config pointed at `server_url`, and the temp dir to clean up.
async fn setup_conflict_job(
    server_url: String,
    label: &str,
) -> (
    Arc<ConfigStore>,
    crate::models::JobConfig,
    ServerConfig,
    PathBuf,
) {
    let temp_dir = temp_test_dir(label);
    let config_dir = temp_dir.join("config");
    let source_path = temp_dir.join("source.csv");
    std::fs::write(&source_path, "placeholder").expect("write source placeholder");

    let config_store = Arc::new(ConfigStore::new(config_dir));
    config_store.ensure().expect("ensure config store");
    let server_config = sample_server(server_url);
    config_store
        .set_server(server_config.clone(), "Test Workspace")
        .expect("set server");
    let job = config_store
        .create_job(sample_job_request(
            source_path.to_str().expect("utf-8 path"),
        ))
        .expect("create job");

    (config_store, job, server_config, temp_dir)
}

fn utc(hour: u32, minute: u32) -> DateTime<Utc> {
    chrono::NaiveDate::from_ymd_opt(2026, 4, 3)
        .unwrap()
        .and_hms_opt(hour, minute, 0)
        .unwrap()
        .and_utc()
}

/// A straddling batch after a crash-retry: some rows are already on the server,
/// some are not. The server rejects the whole batch (409, all-or-nothing). The
/// uploader must advance the cursor only to the server's confirmed watermark and
/// re-send the rows beyond it — never silently skipping the unstored rows.
#[tokio::test]
async fn conflict_advances_to_server_watermark_and_resends_missing_rows() {
    // POST -> 409, GET watermark -> 200, re-POST of the missing row -> 201.
    let server = TestObservationServer::spawn_with_get_body(
        vec![409, 200, 201],
        json!({ "phenomenonEndTime": "2026-04-03T08:00:00Z" }).to_string(),
    )
    .await;
    let (config_store, job, server_config, temp_dir) =
        setup_conflict_job(server.base_url.clone(), "conflict-resend").await;

    let (tx, rx) = bounded(8);
    let worker = spawn_upload_worker(
        rx,
        Arc::new(HydroServerService::new().expect("hydroserver service")),
        config_store.clone(),
    );

    let context = Arc::new(ObservationContext {
        server: Arc::new(server_config),
        job_id: job.id.clone(),
        datastream_id: "ds-1".to_string(),
        datastream_name: "Stage".to_string(),
    });

    // Row 4 @ 08:00 is at the watermark (confirmed); row 5 @ 08:05 is beyond it.
    tx.send(QueuedObservation {
        context: context.clone(),
        timestamp: utc(8, 0),
        row_index: 4,
        value: json!(2.41),
    })
    .await
    .expect("send confirmed observation");
    tx.send(QueuedObservation {
        context,
        timestamp: utc(8, 5),
        row_index: 5,
        value: json!(2.45),
    })
    .await
    .expect("send pending observation");
    drop(tx);

    timeout(Duration::from_secs(10), worker)
        .await
        .expect("worker timeout")
        .expect("join worker");

    assert_eq!(
        server.request_count(),
        3,
        "expected POST(409), GET(watermark), and re-POST of the missing row"
    );

    let bodies = server.bodies().await;
    assert!(
        bodies[2].contains("08:05:00") && !bodies[2].contains("08:00:00"),
        "the re-sent batch should contain only the row beyond the watermark"
    );

    let cursor = config_store.cursor_for(&job.id).expect("load cursor");
    assert_eq!(
        cursor.last_pushed_row_index,
        Some(5),
        "cursor must advance over the re-sent row, not stop at the watermark"
    );
    assert_eq!(cursor.last_pushed_timestamp, Some(utc(8, 5)));
    assert_eq!(cursor.last_error, None);

    server.shutdown().await;
    let _ = std::fs::remove_dir_all(temp_dir);
}

/// A batch that is entirely already on the server: reconciliation should advance
/// the cursor (so it isn't retried forever) without re-sending anything.
#[tokio::test]
async fn pure_duplicate_batch_reconciles_without_resending() {
    let server = TestObservationServer::spawn_with_get_body(
        vec![409, 200],
        json!({ "phenomenonEndTime": "2026-04-03T09:00:00Z" }).to_string(),
    )
    .await;
    let (config_store, job, server_config, temp_dir) =
        setup_conflict_job(server.base_url.clone(), "conflict-pure-dup").await;

    let (tx, rx) = bounded(8);
    let worker = spawn_upload_worker(
        rx,
        Arc::new(HydroServerService::new().expect("hydroserver service")),
        config_store.clone(),
    );

    let context = Arc::new(ObservationContext {
        server: Arc::new(server_config),
        job_id: job.id.clone(),
        datastream_id: "ds-1".to_string(),
        datastream_name: "Stage".to_string(),
    });

    tx.send(QueuedObservation {
        context: context.clone(),
        timestamp: utc(8, 0),
        row_index: 4,
        value: json!(2.41),
    })
    .await
    .expect("send observation");
    tx.send(QueuedObservation {
        context,
        timestamp: utc(8, 5),
        row_index: 5,
        value: json!(2.45),
    })
    .await
    .expect("send observation");
    drop(tx);

    timeout(Duration::from_secs(10), worker)
        .await
        .expect("worker timeout")
        .expect("join worker");

    assert_eq!(
        server.request_count(),
        2,
        "a fully-duplicate batch should only POST once and GET the watermark — no re-send"
    );

    let cursor = config_store.cursor_for(&job.id).expect("load cursor");
    assert_eq!(cursor.last_pushed_row_index, Some(5));
    assert_eq!(cursor.last_pushed_timestamp, Some(utc(8, 5)));
    assert_eq!(cursor.last_error, None);

    server.shutdown().await;
    let _ = std::fs::remove_dir_all(temp_dir);
}

/// If a conflict occurs but the server's watermark can't be fetched, the uploader
/// must NOT advance the cursor — it records a failure and retries later, so no
/// data is lost.
#[tokio::test]
async fn conflict_with_unconfirmable_server_state_does_not_advance_cursor() {
    // POST -> 409, GET watermark -> 500 (cannot confirm).
    let server = TestObservationServer::spawn(vec![409, 500]).await;
    let (config_store, job, server_config, temp_dir) =
        setup_conflict_job(server.base_url.clone(), "conflict-unconfirmable").await;

    let (tx, rx) = bounded(8);
    let worker = spawn_upload_worker(
        rx,
        Arc::new(HydroServerService::new().expect("hydroserver service")),
        config_store.clone(),
    );

    let context = Arc::new(ObservationContext {
        server: Arc::new(server_config),
        job_id: job.id.clone(),
        datastream_id: "ds-1".to_string(),
        datastream_name: "Stage".to_string(),
    });

    tx.send(QueuedObservation {
        context,
        timestamp: utc(8, 0),
        row_index: 4,
        value: json!(2.41),
    })
    .await
    .expect("send observation");
    drop(tx);

    timeout(Duration::from_secs(10), worker)
        .await
        .expect("worker timeout")
        .expect("join worker");

    let cursor = config_store.cursor_for(&job.id).expect("load cursor");
    assert_eq!(
        cursor.last_pushed_row_index, None,
        "cursor must not advance when the server's stored range is unknown"
    );
    assert!(
        cursor.last_error.is_some(),
        "an unreconcilable conflict should be recorded as an error for retry"
    );

    server.shutdown().await;
    let _ = std::fs::remove_dir_all(temp_dir);
}
