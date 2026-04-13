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
