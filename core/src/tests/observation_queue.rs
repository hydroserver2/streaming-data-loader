use super::*;
use serde_json::json;
use std::sync::Arc;

fn test_observation() -> QueuedObservation {
    QueuedObservation {
        context: Arc::new(ObservationContext {
            server: Arc::new(crate::models::ServerConfig::default()),
            job_id: "test".to_string(),
            datastream_id: "ds-1".to_string(),
            datastream_name: "Test".to_string(),
        }),
        timestamp: chrono::Utc::now(),
        row_index: 1,
        value: json!(1.0),
    }
}

#[tokio::test]
async fn bounded_queue_respects_capacity() {
    let (tx, mut rx) = bounded(3);

    // Fill the queue to capacity
    tx.send(test_observation()).await.expect("send 1");
    tx.send(test_observation()).await.expect("send 2");
    tx.send(test_observation()).await.expect("send 3");

    // The 4th send should not complete immediately (channel full)
    let tx_clone = tx.clone();
    let handle = tokio::spawn(async move { tx_clone.send(test_observation()).await });

    // Give the spawn a moment to attempt the send
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !handle.is_finished(),
        "send should block when queue is full"
    );

    // Drain one item to unblock
    rx.recv().await.expect("recv");
    let result = handle.await.expect("join");
    assert!(result.is_ok(), "send should succeed after space freed");
}
