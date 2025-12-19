#![allow(async_fn_in_trait)]
pub mod memory;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InFlightTask {
    pub id: String,
    pub data: Vec<u8>,
    pub consumed_at: DateTime<Utc>,
    pub queue_name: String,
}

pub trait Storage {
    async fn create_queue(&self, name: String) -> Result<(), String>;
    async fn queue(&self, queue_name: &str, item: Vec<u8>) -> Result<(), String>;
    async fn dequeue(&self, queue_name: &str) -> Result<Option<InFlightTask>, String>;
    async fn ack(&self, task_id: &str) -> Result<(), String>;
    async fn nack(&self, task_id: &str) -> Result<(), String>;
    async fn cleanup_expired_tasks(&self, timeout_seconds: u64) -> Result<usize, String>;
    async fn get_stats(&self) -> Result<serde_json::Value, String>;
}
