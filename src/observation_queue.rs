use chrono::{DateTime, Utc};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::models::ServerConfig;

#[derive(Debug, Clone)]
pub struct ObservationContext {
    pub server: Arc<ServerConfig>,
    pub job_id: String,
    pub datastream_id: String,
    pub datastream_name: String,
}

#[derive(Debug, Clone)]
pub struct QueuedObservation {
    pub context: Arc<ObservationContext>,
    pub timestamp: DateTime<Utc>,
    pub row_index: u64,
    pub value: Value,
}

#[derive(Clone)]
pub struct ObservationSender {
    inner: mpsc::Sender<QueuedObservation>,
}

pub struct ObservationReceiver {
    inner: mpsc::Receiver<QueuedObservation>,
}

pub fn bounded(capacity: usize) -> (ObservationSender, ObservationReceiver) {
    let (tx, rx) = mpsc::channel(capacity);
    (
        ObservationSender { inner: tx },
        ObservationReceiver { inner: rx },
    )
}

impl ObservationSender {
    pub async fn send(&self, observation: QueuedObservation) -> Result<(), String> {
        self.inner
            .send(observation)
            .await
            .map_err(|_| "Observation queue is shutting down.".to_string())
    }
}

impl ObservationReceiver {
    pub async fn recv(&mut self) -> Option<QueuedObservation> {
        self.inner.recv().await
    }
}

#[cfg(test)]
mod tests {
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
        let handle = tokio::spawn(async move {
            tx_clone.send(test_observation()).await
        });

        // Give the spawn a moment to attempt the send
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!handle.is_finished(), "send should block when queue is full");

        // Drain one item to unblock
        rx.recv().await.expect("recv");
        let result = handle.await.expect("join");
        assert!(result.is_ok(), "send should succeed after space freed");
    }
}
