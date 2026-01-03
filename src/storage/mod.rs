#![allow(async_fn_in_trait)]
pub mod memory;
pub mod wal;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a task that has been dequeued but not yet acknowledged.
///
/// This struct holds the task data and metadata required for processing and lifecycle management
/// (e.g., timeouts, retries).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InFlightTask {
    /// Unique identifier for the task instance.
    pub id: String,
    /// The actual payload of the task.
    pub data: Vec<u8>,
    /// The timestamp when the task was dequeued/consumed.
    pub consumed_at: DateTime<Utc>,
    /// The name of the queue this task belongs to.
    pub queue_name: String,
    /// Number of times this task has been delivered (for DLQ logic).
    pub delivery_attempts: u32,
}

pub trait Storage {
    /// Creates a new queue with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The unique name of the queue to create.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if created successfully, or an error if the queue already exists.
    async fn create_queue(&self, name: String) -> Result<(), String>;

    /// Adds an item to the specified queue.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The name of the target queue.
    /// * `item` - The binary payload to enqueue.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the queue does not exist.
    async fn queue(&self, queue_name: &str, item: Vec<u8>) -> Result<(), String>;

    /// Retrieves the next item from the queue, moving it to in-flight state.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The name of the queue to dequeue from.
    ///
    /// # Returns
    ///
    /// Returns:
    /// * `Ok(Some(InFlightTask))` if a task is available.
    /// * `Ok(None)` if the queue is empty.
    /// * `Err` if the queue does not exist.
    async fn dequeue(&self, queue_name: &str) -> Result<Option<InFlightTask>, String>;

    /// Acknowledges a task, permanently removing it from the in-flight state.
    ///
    /// This function indicates that the consumer has successfully processed the task.
    /// Once acknowledged, the task cannot be re-queued or seen again.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique ID of the task to acknowledge.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the task is not found (e.g., expired or already acked).
    async fn ack(&self, task_id: &str) -> Result<(), String>;

    /// Negatively acknowledges a task, returning it to the queue for retry.
    ///
    /// If the task has exceeded the maximum delivery attempts, it may be moved to a Dead Letter Queue (DLQ).
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique ID of the task to nack.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the task is not found.
    async fn nack(&self, task_id: &str) -> Result<(), String>;

    /// Extends the visibility timeout for an in-flight task.
    ///
    /// Prevents the task from being re-queued by the background cleanup process
    /// while the consumer is still working on it.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique ID of the task.
    /// * `duration_seconds` - The duration to extend the timeout by.
    async fn extend_visibility(&self, task_id: &str, duration_seconds: u64) -> Result<(), String>;

    /// Cleans up tasks that have been in-flight for longer than the timeout period.
    ///
    /// Expired tasks are either re-queued or moved to a DLQ depending on retry counts.
    ///
    /// # Arguments
    ///
    /// * `timeout_seconds` - The threshold for considering a task expired.
    ///
    /// # Returns
    ///
    /// Returns the number of tasks that were processed/cleaned up.
    async fn cleanup_expired_tasks(&self, timeout_seconds: u64) -> Result<usize, String>;

    /// Returns statistics about the system, including queue depths and in-flight counts.
    async fn get_stats(&self) -> Result<serde_json::Value, String>;

    /// Adds multiple items to a queue in a single operation.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The target queue name.
    /// * `items` - A list of binary payloads to enqueue.
    async fn queue_batch(&self, queue_name: &str, items: Vec<Vec<u8>>) -> Result<(), String>;

    /// Retrieves multiple tasks from a queue in a single operation.
    ///
    /// # Arguments
    ///
    /// * `queue_name` - The source queue name.
    /// * `count` - The maximum number of tasks to retrieve.
    ///
    /// # Returns
    ///
    /// A vector of `InFlightTask` objects. Returns an empty vector if queue is empty.
    async fn dequeue_batch(
        &self,
        queue_name: &str,
        count: usize,
    ) -> Result<Vec<InFlightTask>, String>;

    /// Acknowledges multiple tasks in a single operation.
    ///
    /// # Arguments
    ///
    /// * `task_ids` - A list of task IDs to acknowledge.
    async fn ack_batch(&self, task_ids: Vec<String>) -> Result<(), String>;
}
