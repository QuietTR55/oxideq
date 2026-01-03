use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use openraft::raft::VoteRequest;
use oxideq::app;
use tower::ServiceExt;

#[tokio::test]
async fn test_raft_endpoints_uninitialized() {
    // App without Raft (None)
    let app = app(
        std::sync::Arc::new(oxideq::storage::memory::InMemoryStorage::new()),
        "test_password".to_string(),
        5,
        30,
        60,
        None,
    )
    .await;

    // 1. Vote
    let vote_req = VoteRequest::new(openraft::Vote::new(1, 1), None);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raft/vote")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(serde_json::to_vec(&vote_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Expect 503
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes.as_ref(), b"Raft not initialized");

    // 2. Append Entries
    let append_req = openraft::raft::AppendEntriesRequest::<oxideq::replication::types::TypeConfig> {
        vote: openraft::Vote::new(1, 1),
        prev_log_id: None,
        entries: vec![],
        leader_commit: None,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raft/append")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(serde_json::to_vec(&append_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes.as_ref(), b"Raft not initialized");

    // 3. Install Snapshot
    let snapshot_req = openraft::raft::InstallSnapshotRequest::<
        oxideq::replication::types::TypeConfig,
    > {
        vote: openraft::Vote::new(1, 1),
        meta: openraft::SnapshotMeta {
            last_log_id: None,
            last_membership: openraft::StoredMembership::new(None, openraft::Membership::default()),
            snapshot_id: "test".to_string(),
        },
        offset: 0,
        data: vec![],
        done: true,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/raft/snapshot")
                .header("Content-Type", "application/json")
                .header("x-oxideq-password", "test_password")
                .body(Body::from(serde_json::to_vec(&snapshot_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body_bytes.as_ref(), b"Raft not initialized");
}
