use super::{InFlightTask, Storage};
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct InMemoryStorage {
    queues: Mutex<HashMap<String, VecDeque<Vec<u8>>>>,
    in_flight: Mutex<HashMap<String, InFlightTask>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            queues: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
        }
    }
}

impl Storage for InMemoryStorage {
    async fn create_queue(&self, name: String) -> Result<(), String> {
        let mut queues = self.queues.lock().await;
        if queues.contains_key(&name) {
            return Err(format!("Queue '{}' already exists", name));
        }
        queues.insert(name, VecDeque::new());
        Ok(())
    }

    async fn queue(&self, queue_name: &str, item: Vec<u8>) -> Result<(), String> {
        let mut queues = self.queues.lock().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;
        queue.push_back(item);
        Ok(())
    }

    async fn dequeue(&self, queue_name: &str) -> Result<Option<InFlightTask>, String> {
        let mut queues = self.queues.lock().await;
        let queue = queues
            .get_mut(queue_name)
            .ok_or_else(|| format!("Queue '{}' not found", queue_name))?;

        if let Some(data) = queue.pop_front() {
            let task = InFlightTask {
                id: Uuid::new_v4().to_string(),
                data,
                consumed_at: Utc::now(),
                queue_name: queue_name.to_string(),
            };

            let mut in_flight = self.in_flight.lock().await;
            in_flight.insert(task.id.clone(), task.clone());
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    async fn ack(&self, task_id: &str) -> Result<(), String> {
        let mut in_flight = self.in_flight.lock().await;
        if in_flight.remove(task_id).is_some() {
            Ok(())
        } else {
            Err("Task not found".to_string())
        }
    }

    async fn nack(&self, task_id: &str) -> Result<(), String> {
        let mut in_flight = self.in_flight.lock().await;
        if let Some(task) = in_flight.remove(task_id) {
            let mut queues = self.queues.lock().await;
            // If queue doesn't exist anymore, we effectively drop the task or recreate queue?
            // Spec doesn't say. Assuming queue exists or we error/log.
            // "re-queue to original queue".
            if let Some(queue) = queues.get_mut(&task.queue_name) {
                queue.push_front(task.data);
                Ok(())
            } else {
                Err(format!("Original queue '{}' not found", task.queue_name))
            }
        } else {
            Err("Task not found".to_string())
        }
    }

    async fn cleanup_expired_tasks(&self, timeout_seconds: u64) -> Result<usize, String> {
        let mut in_flight = self.in_flight.lock().await;
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(timeout_seconds as i64);

        let mut expired_ids = Vec::new();
        for (id, task) in in_flight.iter() {
            if now.signed_duration_since(task.consumed_at) > timeout {
                expired_ids.push(id.clone());
            }
        }

        let mut count = 0;
        let mut queues = self.queues.lock().await;

        for id in expired_ids {
            if let Some(task) = in_flight.remove(&id) {
                // Requeue
                if let Some(queue) = queues.get_mut(&task.queue_name) {
                    queue.push_front(task.data);
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    async fn get_stats(&self) -> Result<serde_json::Value, String> {
        let queues = self.queues.lock().await;
        let in_flight = self.in_flight.lock().await;

        let total_in_flight = in_flight.len();

        let mut queue_stats = serde_json::Map::new();
        for (name, queue) in queues.iter() {
            queue_stats.insert(
                name.clone(),
                serde_json::json!({
                    "pending": queue.len(),
                    // Counting in-flight per queue requires iterating, might be slow.
                    // Spec says "Per-queue: in-flight count".
                    "in_flight": in_flight.values().filter(|t| t.queue_name == *name).count()
                }),
            );
        }

        Ok(serde_json::json!({
            "total_in_flight": total_in_flight,
            "queues": queue_stats
        }))
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
}
