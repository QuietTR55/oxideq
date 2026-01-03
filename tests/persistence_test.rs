use oxideq::storage::Storage;
use oxideq::storage::memory::InMemoryStorage;
use std::fs;
use uuid::Uuid;

#[tokio::test]
async fn test_persistence_workflow() {
    let wal_path = format!("test_wal_{}.log", Uuid::new_v4());

    // 1. Create storage with WAL, create queue, enqueue item
    {
        let storage = InMemoryStorage::new_persistent(&wal_path).await;
        storage
            .create_queue("persist_queue".to_string())
            .await
            .unwrap();
        storage
            .queue("persist_queue", b"persist_me".to_vec())
            .await
            .unwrap();
        // Storage dropped here
    }

    // 2. Load again from same WAL
    {
        let storage = InMemoryStorage::new_persistent(&wal_path).await;
        // Verify queue exists after reload
        let stats = storage.get_stats().await.unwrap();
        let queues = stats.get("queues").unwrap().as_object().unwrap();
        assert!(
            queues.contains_key("persist_queue"),
            "Queue should exist after reload"
        );

        // Dequeue
        let task_opt = storage.dequeue("persist_queue").await.unwrap();
        assert!(task_opt.is_some(), "Should restore enqueued item");
        let task = task_opt.unwrap();
        assert_eq!(task.data, b"persist_me");
    }

    // Cleanup
    let _ = fs::remove_file(wal_path);
}
