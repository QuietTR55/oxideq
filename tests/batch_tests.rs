use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use oxideq::app;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

#[tokio::test]
async fn test_batch_json_workflow() {
    let app = app(std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()), "test_password".to_string(), 5, 30, 60, None, CancellationToken::new()).await;
    let queue_name = "batch_json_queue";

    // Create Queue
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": queue_name })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    // Batch Enqueue (JSON)
    let payload = json!([
        { "msg": "task1" },
        { "msg": "task2" }
    ]);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks/batch", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&payload).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Batch Dequeue (JSON)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/queues/{}/tasks/batch?count=5", queue_name))
                .header("Accept", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let tasks: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(tasks.len(), 2);
    // Tasks are { id: "...", message: { "msg": "..." } }
    assert_eq!(tasks[0]["message"]["msg"], "task1");
    assert_eq!(tasks[1]["message"]["msg"], "task2");

    let task_ids: Vec<String> = tasks
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();

    // Batch ACK
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks/ack/batch", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&task_ids).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Verify empty
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/queues/{}/tasks/batch", queue_name))
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let tasks: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(tasks.len(), 0);
}

#[tokio::test]
async fn test_batch_msgpack_workflow() {
    let app = app(std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()), "test_password".to_string(), 5, 30, 60, None, CancellationToken::new()).await;
    let queue_name = "batch_msgpack_queue";

    // Create Queue
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": queue_name })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    // Batch Enqueue (MsgPack)
    let payload_vec = vec![json!({ "foo": "bar" }), json!({ "baz": "qux" })];
    let payload = rmp_serde::to_vec(&payload_vec).unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks/batch", queue_name))
                .header("Content-Type", "application/msgpack") // Magic header
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(payload)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Batch Dequeue (MsgPack)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/queues/{}/tasks/batch", queue_name))
                .header("Accept", "application/msgpack")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/msgpack");

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();

    // Deserialize MsgPack response
    // Response is Array of { id, message }
    #[derive(serde::Deserialize, Debug)]
    struct BatchItem {
        #[allow(dead_code)]
        id: String,
        message: serde_json::Value,
    }

    let items: Vec<BatchItem> = rmp_serde::from_slice(&body_bytes).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].message["foo"], "bar");
    assert_eq!(items[1].message["baz"], "qux");
}

#[tokio::test]
async fn test_single_binary_protocol() {
    // Tests that normal enqueue/dequeue supports binary (bytes) via pass-through
    let app = app(std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()), "test_password".to_string(), 5, 30, 60, None, CancellationToken::new()).await;
    let queue_name = "binary_queue";

    // Create Queue
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": queue_name })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    // Enqueue Raw Bytes (Not valid UTF8)
    let raw_bytes = vec![0u8, 159, 146, 150];
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(raw_bytes.clone())))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Dequeue Raw Bytes
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes.to_vec(), raw_bytes);
}
