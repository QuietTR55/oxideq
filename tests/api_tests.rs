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
async fn test_create_queue() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": "integration_test_queue" })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_enqueue_dequeue() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;
    let queue_name = "task_queue";

    // Create queue first
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

    // Enqueue
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "message": "hello world" })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Dequeue
    let response = app
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(
        body_str,
        serde_json::to_string(&json!({ "message": "hello world" })).unwrap()
    );
}

#[tokio::test]
async fn test_unauthorized() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "secret_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;

    // No header
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": "forbidden_queue" })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Wrong password
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queues")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "wrong_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&json!({ "name": "forbidden_queue" })).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_create_task_with_extra_fields() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;
    let queue_name = "repro_queue";

    // Create queue
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

    let task_payload = json!({
        "message": "task",
        "second_message": "task 2",
        "object_message": {
            "object_message_content":"object_message_content_text"
        }
    });

    // Enqueue
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(Bytes::from(
                    serde_json::to_vec(&task_payload).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Dequeue
    let response = app
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(
        body_str,
        serde_json::to_string(&task_payload).unwrap(),
        "Payload mismatch! Expected full JSON, got: {}",
        body_str
    );
}

#[tokio::test]
async fn test_ack_workflow() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;
    let queue_name = "ack_queue";

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

    // Enqueue
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from("test-task".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Dequeue
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
    let task_id = response
        .headers()
        .get("x-oxideq-task-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(!task_id.is_empty());

    // Verify Stats (1 in-flight)
    let stats_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/stats")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body: serde_json::Value =
        serde_json::from_slice(&stats_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(stats_body["total_in_flight"], 1);

    // ACK
    let ack_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks/{}/ack", queue_name, task_id))
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ack_res.status(), StatusCode::OK);

    // Verify Stats (0 in-flight)
    let stats_res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/stats")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body2: serde_json::Value =
        serde_json::from_slice(&stats_res2.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(stats_body2["total_in_flight"], 0);
}

#[tokio::test]
async fn test_nack_workflow() {
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
        CancellationToken::new(),
    )
    .await;
    let queue_name = "nack_queue";

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

    // Enqueue
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from("test-task".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Dequeue
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
    let task_id = response
        .headers()
        .get("x-oxideq-task-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // NACK
    let nack_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks/{}/nack", queue_name, task_id))
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(nack_res.status(), StatusCode::OK);

    // Verify Stats (0 in-flight)
    let stats_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/stats")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body: serde_json::Value =
        serde_json::from_slice(&stats_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(stats_body["total_in_flight"], 0);

    // Dequeue again (should get same message)
    let response2 = app
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
    assert_eq!(response2.status(), StatusCode::OK);
    let body_str = String::from_utf8(
        response2
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(body_str, "test-task");
}

#[tokio::test]
async fn test_timeout_cleanup() {
    // 1 second cleanup interval, 1 second timeout
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        1,
        1,
        60,
        None,
        CancellationToken::new(),
    )
    .await;
    let queue_name = "timeout_queue";

    // Create & Enqueue
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
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/queues/{}/tasks", queue_name))
                .header("x-oxideq-password", "test_password")
                .body(Body::from("test-task".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Dequeue
    let _ = app
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

    // Verify in-flight
    let stats_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/stats")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body: serde_json::Value =
        serde_json::from_slice(&stats_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(stats_body["total_in_flight"], 1);

    // Wait for cleanup (1.5s > 1s)
    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

    // Check stats again (retry if needed is removed for simplicity, 2.5s should be enough)
    let stats_res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/stats")
                .header("x-oxideq-password", "test_password")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body2: serde_json::Value =
        serde_json::from_slice(&stats_res2.into_body().collect().await.unwrap().to_bytes())
            .unwrap();

    assert_eq!(
        stats_body2["total_in_flight"], 0,
        "Task should have been cleaned up"
    );
}
