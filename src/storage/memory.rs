use super::{InFlightTask, Storage};
use crate::storage::wal::{WalEntry, WalOp, manager::WalManager};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub data: Vec<u8>,
    pub delivery_attempts: u32,
}

impl InMemoryStorage {
    pub fn get_snapshot_data(
        &self,
    ) -> (
        HashMap<String, VecDeque<QueueItem>>,
        HashMap<String, InFlightTask>,
    ) {
        let queues: HashMap<_, _> = self
            .queues
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();
        let in_flight: HashMap<_, _> = self
            .in_flight
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();
        (queues, in_flight)
    }

    pub fn scan_expired_tasks(&self, timeout_seconds: u64) -> Vec<String> {
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(timeout_seconds as i64);

        let mut expired_ids = Vec::new();
        for r in self.in_flight.iter() {
            if now.signed_duration_since(r.value().consumed_at) > timeout {
                expired_ids.push(r.key().clone());
            }
        }
        expired_ids
    }

    pub fn restore_from_snapshot(
        &self,
        queues: HashMap<String, VecDeque<QueueItem>>,
        in_flight: HashMap<String, InFlightTask>,
    ) {
        self.queues.clear();
        for (k, v) in queues {
            self.queues.insert(k, v);
        }

        self.in_flight.clear();
        for (k, v) in in_flight {
            self.in_flight.insert(k, v);
        }
    }
}

#[derive(Debug)]
pub struct InMemoryStorage {
    queues: DashMap<String, VecDeque<QueueItem>>,
    in_flight: DashMap<String, InFlightTask>,
    wal: Option<Mutex<WalManager>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            queues: DashMap::new(),
            in_flight: DashMap::new(),
            wal: None,
        }
    }

    pub async fn new_persistent(path: &str) -> Self {
        // Replay
        let entries = WalManager::replay(path).await.unwrap_or_default();
        let storage = Self::new();

        for entry in entries {
            match entry.op {
                WalOp::CreateQueue(name) => {
                    let _ = storage.create_queue(name).await;
                }
                WalOp::Enqueue { queue, data } => {
                    let _ = storage.queue(&queue, data).await;
                }
                WalOp::Dequeued { queue, task_id } => {
                    // Custom logic to force task_id
                    if let Some(mut q) = storage.queues.get_mut(&queue) {
                        if let Some(item) = q.pop_front() {
                            let task = InFlightTask {
                                id: task_id,
                                data: item.data,
                                consumed_at: Utc::now(), // Approximate?
                                queue_name: queue,
                                delivery_attempts: item.delivery_attempts + 1,
                            };
                            storage.in_flight.insert(task.id.clone(), task);
                        }
                    }
                }
                WalOp::Ack(task_id) => {
                    let _ = storage.ack(&task_id).await;
                }
                WalOp::AckBatch(task_ids) => {
                    let _ = storage.ack_batch(task_ids).await;
                }
                WalOp::Nack(task_id) => {
                    let _ = storage.nack(&task_id).await;
                }
                WalOp::ExtendVisibility {
                    task_id,
                    duration: _,
                } => {
                    let _ = storage.extend_visibility(&task_id, 0).await;
                }
                WalOp::EnqueueBatch { queue, data } => {
                    let _ = storage.queue_batch(&queue, data).await;
                }
                WalOp::DequeuedBatch { queue, task_ids } => {
                    if let Some(mut q) = storage.queues.get_mut(&queue) {
                        for task_id in task_ids {
                            if let Some(item) = q.pop_front() {
                                let task = InFlightTask {
                                    id: task_id,
                                    data: item.data,
                                    consumed_at: Utc::now(), // Approximate
                                    queue_name: queue.clone(),
                                    delivery_attempts: item.delivery_attempts + 1,
                                };
                                storage.in_flight.insert(task.id.clone(), task);
                            }
                        }
                    }
                }
            }
        }

        // Initialize WAL manager for appending
        let manager = WalManager::new(path).await;
        Self {
            queues: storage.queues,
            in_flight: storage.in_flight,
            wal: Some(Mutex::new(manager)),
        }
    }
}

impl Storage for InMemoryStorage {
    async fn create_queue(&self, name: String) -> Result<(), String> {
        if self.queues.contains_key(&name) {
            return Err(format!("Queue '{}' already exists", name));
        }

        if let Some(wal) = &self.wal {
            wal.lock()
                .await
                .append(&WalEntry {
                    op: WalOp::CreateQueue(name.clone()),
                    timestamp: Utc::now().timestamp_millis(),
                })
                .await?;
        }

        self.queues.insert(name, VecDeque::new());
        Ok(())
    }

    async fn queue(&self, queue_name: &str, item: Vec<u8>) -> Result<(), String> {
        // Validation first: Queue must exist
        if !self.queues.contains_key(queue_name) {
            return Err(format!("Queue '{}' not found", queue_name));
        }

        if let Some(wal) = &self.wal {
            wal.lock()
                .await
                .append(&WalEntry {
                    op: WalOp::Enqueue {
                        queue: queue_name.to_string(),
                        data: item.clone(),
                    },
                    timestamp: Utc::now().timestamp_millis(),
                })
                .await?;
        }

        let mut queue = self
            .queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;
        queue.push_back(QueueItem {
            data: item,
            delivery_attempts: 0,
        });
        Ok(())
    }

    async fn dequeue(&self, queue_name: &str) -> Result<Option<InFlightTask>, String> {
        let mut queue = self
            .queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;

        if let Some(item) = queue.pop_front() {
            let task_id = Uuid::new_v4().to_string();

            // Log to WAL
            if let Some(wal) = &self.wal {
                if let Err(e) = wal
                    .lock()
                    .await
                    .append(&WalEntry {
                        op: WalOp::Dequeued {
                            queue: queue_name.to_string(),
                            task_id: task_id.clone(),
                        },
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await
                {
                    // Compensation: Restore item to queue
                    queue.push_front(item);
                    return Err(e);
                }
            }

            let task = InFlightTask {
                id: task_id,
                data: item.data,
                consumed_at: Utc::now(),
                queue_name: queue_name.to_string(),
                delivery_attempts: item.delivery_attempts + 1,
            };

            self.in_flight.insert(task.id.clone(), task.clone());
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    async fn ack(&self, task_id: &str) -> Result<(), String> {
        // Atomically remove the task first to avoid TOCTOU race
        if let Some((_, _task)) = self.in_flight.remove(task_id) {
            // Task found and removed, now log to WAL
            if let Some(wal) = &self.wal {
                if let Err(e) = wal
                    .lock()
                    .await
                    .append(&WalEntry {
                        op: WalOp::Ack(task_id.to_string()),
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await
                {
                    // WAL write failed - this is a critical error
                    // The task is already removed from memory, so we can't restore it
                    // Log and return error, but the ack is effectively complete
                    eprintln!("WAL write failed for ack {}: {}", task_id, e);
                    return Err(format!("WAL write failed: {}", e));
                }
            }
            Ok(())
        } else {
            Err("Task not found".to_string())
        }
    }

    async fn nack(&self, task_id: &str) -> Result<(), String> {
        // Atomically remove the task first to avoid TOCTOU race
        if let Some((_, task)) = self.in_flight.remove(task_id) {
            // Task found and removed, now log to WAL
            if let Some(wal) = &self.wal {
                if let Err(e) = wal
                    .lock()
                    .await
                    .append(&WalEntry {
                        op: WalOp::Nack(task_id.to_string()),
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await
                {
                    // WAL write failed - restore the task to in-flight
                    self.in_flight.insert(task_id.to_string(), task);
                    return Err(format!("WAL write failed: {}", e));
                }
            }

            // Re-queue the task or move to DLQ
            if task.delivery_attempts >= 5 {
                let dlq_name = format!("{}_dlq", task.queue_name);
                // DLQ is created implicitly - on WAL replay, nack logic will recreate it
                let mut queue = self.queues.entry(dlq_name).or_insert_with(VecDeque::new);
                queue.push_back(QueueItem {
                    data: task.data,
                    delivery_attempts: task.delivery_attempts,
                });
            } else {
                if let Some(mut queue) = self.queues.get_mut(&task.queue_name) {
                    queue.push_front(QueueItem {
                        data: task.data,
                        delivery_attempts: task.delivery_attempts,
                    });
                } else {
                    return Err(format!("Original queue '{}' not found", task.queue_name));
                }
            }
            Ok(())
        } else {
            Err("Task not found".to_string())
        }
    }

    async fn extend_visibility(&self, task_id: &str, duration_seconds: u64) -> Result<(), String> {
        // Get mutable reference atomically to avoid TOCTOU race
        if let Some(mut task) = self.in_flight.get_mut(task_id) {
            // Log to WAL first for durability
            if let Some(wal) = &self.wal {
                wal.lock()
                    .await
                    .append(&WalEntry {
                        op: WalOp::ExtendVisibility {
                            task_id: task_id.to_string(),
                            duration: duration_seconds,
                        },
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await?;
            }

            task.consumed_at = Utc::now();
            Ok(())
        } else {
            Err("Task not found".to_string())
        }
    }

    async fn cleanup_expired_tasks(&self, timeout_seconds: u64) -> Result<usize, String> {
        let expired_ids = self.scan_expired_tasks(timeout_seconds);
        let mut count = 0;

        for id in expired_ids {
            // We use nack to handle the requeue/DLQ logic and WAL logging safely
            if let Ok(_) = self.nack(&id).await {
                count += 1;
            }
        }

        Ok(count)
    }

    async fn get_stats(&self) -> Result<serde_json::Value, String> {
        let total_in_flight = self.in_flight.len();

        let mut queue_stats = serde_json::Map::new();
        for r in self.queues.iter() {
            let name = r.key();
            let queue = r.value();
            queue_stats.insert(
                name.clone(),
                serde_json::json!({
                    "pending": queue.len(),
                    /*
                    In-flight count per queue. Iterating over all in_flight is O(N).
                    */
                    "in_flight": self.in_flight.iter().filter(|t| t.value().queue_name == *name).count()
                }),
            );
        }

        Ok(serde_json::json!({
            "total_in_flight": total_in_flight,
            "queues": queue_stats
        }))
    }

    async fn queue_batch(&self, queue_name: &str, items: Vec<Vec<u8>>) -> Result<(), String> {
        if !self.queues.contains_key(queue_name) {
            return Err(format!("Queue '{}' not found", queue_name));
        }

        if let Some(wal) = &self.wal {
            wal.lock()
                .await
                .append(&WalEntry {
                    op: WalOp::EnqueueBatch {
                        queue: queue_name.to_string(),
                        data: items.clone(),
                    },
                    timestamp: Utc::now().timestamp_millis(),
                })
                .await?;
        }

        let mut queue = self
            .queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;

        for item in items {
            queue.push_back(QueueItem {
                data: item,
                delivery_attempts: 0,
            });
        }
        Ok(())
    }

    async fn dequeue_batch(
        &self,
        queue_name: &str,
        count: usize,
    ) -> Result<Vec<InFlightTask>, String> {
        let mut queue = self
            .queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;

        if queue.is_empty() {
            return Ok(Vec::new());
        }

        let mut popped_items = Vec::with_capacity(count);

        for _ in 0..count {
            if let Some(item) = queue.pop_front() {
                popped_items.push(item);
            } else {
                break;
            }
        }

        if popped_items.is_empty() {
            return Ok(Vec::new());
        }

        let mut task_ids = Vec::with_capacity(popped_items.len());
        for _ in 0..popped_items.len() {
            task_ids.push(Uuid::new_v4().to_string());
        }

        if let Some(wal) = &self.wal {
            if let Err(e) = wal
                .lock()
                .await
                .append(&WalEntry {
                    op: WalOp::DequeuedBatch {
                        queue: queue_name.to_string(),
                        task_ids: task_ids.clone(),
                    },
                    timestamp: Utc::now().timestamp_millis(),
                })
                .await
            {
                // Start restore
                for item in popped_items.into_iter().rev() {
                    queue.push_front(item);
                }
                return Err(e);
            }
        }

        let mut tasks = Vec::with_capacity(task_ids.len());
        for (i, item) in popped_items.into_iter().enumerate() {
            let task_id = task_ids[i].clone();
            let task = InFlightTask {
                id: task_id.clone(),
                data: item.data,
                consumed_at: Utc::now(),
                queue_name: queue_name.to_string(),
                delivery_attempts: item.delivery_attempts + 1,
            };
            self.in_flight.insert(task_id, task.clone());
            tasks.push(task);
        }

        Ok(tasks)
    }

    async fn ack_batch(&self, task_ids: Vec<String>) -> Result<(), String> {
        // First verify all tasks exist to maintain consistency with single ack
        let mut missing_ids = Vec::new();
        for task_id in &task_ids {
            if !self.in_flight.contains_key(task_id) {
                missing_ids.push(task_id.clone());
            }
        }

        if !missing_ids.is_empty() {
            return Err(format!("Tasks not found: {}", missing_ids.join(", ")));
        }

        // All tasks exist, log to WAL
        if let Some(wal) = &self.wal {
            wal.lock()
                .await
                .append(&WalEntry {
                    op: WalOp::AckBatch(task_ids.clone()),
                    timestamp: Utc::now().timestamp_millis(),
                })
                .await?;
        }

        // Remove all tasks
        for task_id in task_ids {
            self.in_flight.remove(&task_id);
        }
        Ok(())
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_queue_dequeue() {
        let storage = InMemoryStorage::new();
        let queue_name = "test_queue";

        storage.create_queue(queue_name.to_string()).await.unwrap();
        let item = b"hello".to_vec();
        storage.queue(queue_name, item.clone()).await.unwrap();

        let result = storage.dequeue(queue_name).await.unwrap();
        assert!(result.is_some());
        let task = result.unwrap();
        assert_eq!(task.data, item);
        assert!(!task.id.is_empty());

        // Check it's in flight
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats["total_in_flight"], 1);

        // Ack
        storage.ack(&task.id).await.unwrap();
        let stats_after = storage.get_stats().await.unwrap();
        assert_eq!(stats_after["total_in_flight"], 0);
    }

    #[tokio::test]
    async fn test_dlq_functionality() {
        let storage = InMemoryStorage::new();
        let queue_name = "dlq_test_queue";
        storage.create_queue(queue_name.to_string()).await.unwrap();

        let item = b"poison".to_vec();
        storage.queue(queue_name, item.clone()).await.unwrap();

        // Retry 5 times (attempts 1 to 5)
        for i in 1..=5 {
            let task_opt = storage.dequeue(queue_name).await.unwrap();
            assert!(task_opt.is_some(), "Should dequeue on attempt {}", i);
            let task = task_opt.unwrap();
            assert_eq!(task.delivery_attempts, i as u32);
            storage.nack(&task.id).await.unwrap();
        }

        // 6th attempt: Should be empty in original queue
        let task_opt = storage.dequeue(queue_name).await.unwrap();
        assert!(task_opt.is_none(), "Original queue should be empty");

        // Check DLQ
        let dlq_name = format!("{}_dlq", queue_name);

        let dlq_task = storage.dequeue(&dlq_name).await.unwrap();
        assert!(dlq_task.is_some(), "Task should be in DLQ");
        assert_eq!(dlq_task.unwrap().data, item);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let storage = InMemoryStorage::new();
        let queue_name = "heartbeat_queue";
        storage.create_queue(queue_name.to_string()).await.unwrap();
        storage.queue(queue_name, b"alive".to_vec()).await.unwrap();

        let task_opt = storage.dequeue(queue_name).await.unwrap();
        let task = task_opt.unwrap();

        let timeout_secs = 2;

        // Wait 1.1s (consume > 50% timeout)
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Send heartbeat
        storage.extend_visibility(&task.id, 60).await.unwrap();

        // Wait another 1.1s. Total 2.2s.
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Should NOT be cleaned up because heartbeat reset the clock
        let cleaned = storage.cleanup_expired_tasks(timeout_secs).await.unwrap();
        assert_eq!(cleaned, 0, "Task should not be cleaned up after heartbeat");

        // Wait more to ensure it eventually expires
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let cleaned_2 = storage.cleanup_expired_tasks(timeout_secs).await.unwrap();
        assert_eq!(cleaned_2, 1, "Task should expire eventually");
    }
}
