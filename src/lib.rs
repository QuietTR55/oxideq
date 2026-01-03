use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Json, Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use subtle::ConstantTimeEq;
pub mod replication;
pub mod storage;
use openraft::BasicNode;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use storage::Storage;
use storage::memory::InMemoryStorage;

use crate::replication::types::{ClientRequest, ClientResponse, NodeId, OxideRaft};
use openraft::error::{ClientWriteError, RaftError};
use openraft::raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest};

#[derive(Clone)]
struct AppState {
    storage: Arc<InMemoryStorage>,
    password: String,
    raft: Option<OxideRaft>,
    heartbeat_duration: u64,
}

#[derive(Deserialize)]
struct CreateQueuePayload {
    name: String,
}

/// Maximum allowed queue name length
const MAX_QUEUE_NAME_LENGTH: usize = 128;

/// Validates a queue name.
/// Returns Ok(()) if valid, Err(error_message) if invalid.
fn validate_queue_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Queue name cannot be empty".to_string());
    }
    if name.len() > MAX_QUEUE_NAME_LENGTH {
        return Err(format!(
            "Queue name exceeds maximum length of {} characters",
            MAX_QUEUE_NAME_LENGTH
        ));
    }
    // Allow alphanumeric, underscores, and hyphens only
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(
            "Queue name can only contain alphanumeric characters, underscores, and hyphens"
                .to_string(),
        );
    }
    Ok(())
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let password_header = req.headers().get("x-oxideq-password");
    match password_header {
        Some(header) => {
            let header_bytes = header.as_bytes();
            let password_bytes = state.password.as_bytes();
            // Use constant-time comparison to prevent timing attacks
            if header_bytes.len() == password_bytes.len()
                && header_bytes.ct_eq(password_bytes).into()
            {
                Ok(next.run(req).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn create_queue(
    State(state): State<AppState>,
    Json(payload): Json<CreateQueuePayload>,
) -> impl IntoResponse {
    // Validate queue name
    if let Err(e) = validate_queue_name(&payload.name) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    if let Some(raft) = &state.raft {
        let req = ClientRequest::CreateQueue { name: payload.name };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::CREATED.into_response(),
                ClientResponse::Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.create_queue(payload.name).await {
            Ok(_) => StatusCode::CREATED.into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
        }
    }
}

async fn enqueue(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
    payload: Bytes,
) -> impl IntoResponse {
    // Validate queue name
    if let Err(e) = validate_queue_name(&queue_name) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let item = payload.to_vec();
    if let Some(raft) = &state.raft {
        let req = ClientRequest::Enqueue {
            queue: queue_name,
            data: item,
        };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.queue(&queue_name, item).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

async fn dequeue(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
) -> impl IntoResponse {
    // Validate queue name
    if let Err(e) = validate_queue_name(&queue_name) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    if let Some(raft) = &state.raft {
        let req = ClientRequest::Dequeue { queue: queue_name };
        match raft.client_write(req).await {
            Ok(repl) => match repl.data {
                ClientResponse::Task(Some(task)) => {
                    let mut response = Bytes::from(task.data).into_response();
                    if let Ok(val) = HeaderValue::from_str(&task.id) {
                        response.headers_mut().insert("x-oxideq-task-id", val);
                    }
                    response
                }
                ClientResponse::Task(None) => StatusCode::NO_CONTENT.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.dequeue(&queue_name).await {
            Ok(Some(task)) => {
                let mut response = Bytes::from(task.data).into_response();
                if let Ok(val) = HeaderValue::from_str(&task.id) {
                    response.headers_mut().insert("x-oxideq-task-id", val);
                }
                response
            }
            Ok(None) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

/// Handler for acknowledging a task.
///
/// Delegates to `Storage::ack` to permanently remove the task from in-flight state.
async fn ack_task(
    State(state): State<AppState>,
    Path((_queue_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        let req = ClientRequest::Ack { task_id };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.ack(&task_id).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

async fn nack_task(
    State(state): State<AppState>,
    Path((_queue_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        let req = ClientRequest::Nack { task_id };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.nack(&task_id).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

async fn get_stats(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.ensure_linearizable().await {
            Ok(_) => {}
            Err(RaftError::APIError(openraft::error::CheckIsLeaderError::ForwardToLeader(err))) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("Not leader. Leader is: {:?}", err.leader_node),
                )
                    .into_response();
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }

    match state.storage.get_stats().await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn heartbeat_task(
    State(state): State<AppState>,
    Path((_queue_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let duration_seconds = state.heartbeat_duration;
    if let Some(raft) = &state.raft {
        let req = ClientRequest::ExtendVisibility {
            task_id,
            duration_seconds,
        };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state
            .storage
            .extend_visibility(&task_id, duration_seconds)
            .await
        {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

#[derive(Deserialize)]
struct BatchQueryParams {
    count: Option<usize>,
}

async fn queue_batch(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Validate queue name
    if let Err(e) = validate_queue_name(&queue_name) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    let items: Vec<Vec<u8>> = if content_type == "application/msgpack" {
        let values: Vec<serde_json::Value> = match rmp_serde::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        values
            .into_iter()
            .map(|v| rmp_serde::to_vec(&v).unwrap())
            .collect()
    } else {
        let values: Vec<serde_json::Value> = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
        values
            .into_iter()
            .map(|v| serde_json::to_vec(&v).unwrap())
            .collect()
    };

    if let Some(raft) = &state.raft {
        let req = ClientRequest::EnqueueBatch {
            queue: queue_name,
            data: items,
        };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.queue_batch(&queue_name, items).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

async fn dequeue_batch(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
    axum::extract::Query(params): axum::extract::Query<BatchQueryParams>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Validate queue name
    if let Err(e) = validate_queue_name(&queue_name) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let count = params.count.unwrap_or(10);

    let tasks_result = if let Some(raft) = &state.raft {
        let req = ClientRequest::DequeueBatch {
            queue: queue_name,
            count,
        };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Tasks(tasks) => Ok(tasks),
                ClientResponse::Err(e) => Err(e),
                _ => Err("Invalid response type".to_string()),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("Not leader. Leader is: {:?}", err.leader_node),
                )
                    .into_response();
            }
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        state.storage.dequeue_batch(&queue_name, count).await
    };

    match tasks_result {
        Ok(tasks) => {
            let accept = headers
                .get(axum::http::header::ACCEPT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json");

            if accept == "application/msgpack" {
                let mut values = Vec::with_capacity(tasks.len());
                let mut ids = Vec::with_capacity(tasks.len());

                for task in tasks {
                    let v: serde_json::Value =
                        if task.data.first().map_or(false, |&b| b == 123 || b == 91) {
                            serde_json::from_slice(&task.data).unwrap_or(serde_json::Value::Null)
                        } else {
                            rmp_serde::from_slice(&task.data).unwrap_or(serde_json::Value::Null)
                        };
                    values.push(v);
                    ids.push(task.id);
                }

                #[derive(serde::Serialize)]
                struct BatchItem {
                    id: String,
                    message: serde_json::Value,
                }

                let response_items: Vec<BatchItem> = values
                    .into_iter()
                    .zip(ids)
                    .map(|(v, id)| BatchItem { id, message: v })
                    .collect();
                let bytes = rmp_serde::to_vec(&response_items).unwrap();
                let mut response = Bytes::from(bytes).into_response();
                response.headers_mut().insert(
                    axum::http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/msgpack"),
                );
                response
            } else {
                #[derive(serde::Serialize)]
                struct BatchItem {
                    id: String,
                    message: serde_json::Value,
                }
                let mut response_items = Vec::with_capacity(tasks.len());
                for task in tasks {
                    let v: serde_json::Value = serde_json::from_slice(&task.data)
                        .or_else(|_| rmp_serde::from_slice(&task.data))
                        .unwrap_or(serde_json::Value::Null);
                    response_items.push(BatchItem {
                        id: task.id,
                        message: v,
                    });
                }
                Json(response_items).into_response()
            }
        }
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn ack_batch(
    State(state): State<AppState>,
    Json(task_ids): Json<Vec<String>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        let req = ClientRequest::AckBatch { task_ids };
        match raft.client_write(req).await {
            Ok(resp) => match resp.data {
                ClientResponse::Ok => StatusCode::OK.into_response(),
                ClientResponse::Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid response type").into_response(),
            },
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(err))) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Not leader. Leader is: {:?}", err.leader_node),
            )
                .into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match state.storage.ack_batch(task_ids).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
        }
    }
}

async fn raft_vote(
    State(state): State<AppState>,
    Json(rpc): Json<VoteRequest<NodeId>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.vote(rpc).await {
            Ok(res) => Json(res).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft vote error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_append(
    State(state): State<AppState>,
    Json(rpc): Json<AppendEntriesRequest<crate::replication::types::TypeConfig>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.append_entries(rpc).await {
            Ok(res) => Json(res).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft append error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_snapshot(
    State(state): State<AppState>,
    Json(rpc): Json<InstallSnapshotRequest<crate::replication::types::TypeConfig>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.install_snapshot(rpc).await {
            Ok(res) => Json(res).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft snapshot error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_init(
    State(state): State<AppState>,
    Json(nodes): Json<BTreeMap<NodeId, BasicNode>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.initialize(nodes).await {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft init error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_add_learner(
    State(state): State<AppState>,
    Json(body): Json<(NodeId, BasicNode)>,
) -> impl IntoResponse {
    let (node_id, node) = body;
    if let Some(raft) = &state.raft {
        match raft.add_learner(node_id, node, true).await {
            Ok(res) => Json(res).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft add learner error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_change_membership(
    State(state): State<AppState>,
    Json(members): Json<BTreeSet<NodeId>>,
) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        match raft.change_membership(members, true).await {
            Ok(res) => Json(res).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Raft change membership error: {}", e),
            )
                .into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Raft not initialized").into_response()
    }
}

async fn raft_metrics(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(raft) = &state.raft {
        let metrics = raft.metrics().borrow().clone();
        Json(metrics).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Raft not initialized".to_string(),
        )
            .into_response()
    }
}

/// Health check endpoint - returns 200 OK if the service is healthy
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    // Basic health check: verify we can access the storage
    match state.storage.get_stats().await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "healthy"}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"status": "unhealthy", "error": e})),
        )
            .into_response(),
    }
}

pub async fn app(
    storage: Arc<InMemoryStorage>,
    password: String,
    cleanup_interval: u64,
    task_timeout: u64,
    heartbeat_duration: u64,
    raft: Option<OxideRaft>,
) -> Router {
    let state = AppState {
        storage: storage.clone(),
        password,
        raft,
        heartbeat_duration,
    };

    // Spawn cleanup task
    let storage_clone = storage.clone();
    let raft_clone = state.raft.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval));
        loop {
            interval.tick().await;

            if let Some(raft) = &raft_clone {
                // Cluster Mode: Only leader initiates cleanup via Raft log
                let is_leader = raft.metrics().borrow().state == openraft::ServerState::Leader;
                if is_leader {
                    let expired = storage_clone.scan_expired_tasks(task_timeout);
                    for id in expired {
                        let req = ClientRequest::Nack {
                            task_id: id.clone(),
                        };
                        if let Err(e) = raft.client_write(req).await {
                            eprintln!("Raft cleanup error for task {}: {}", id, e);
                        }
                    }
                }
            } else {
                // Standalone Mode: Direct cleanup
                if let Err(e) = storage_clone.cleanup_expired_tasks(task_timeout).await {
                    eprintln!("Cleanup error: {}", e);
                }
            }
        }
    });

    // Body size limits: 1MB for single messages, 10MB for batch operations
    const SINGLE_MESSAGE_LIMIT: usize = 1024 * 1024; // 1MB
    const BATCH_MESSAGE_LIMIT: usize = 10 * 1024 * 1024; // 10MB

    Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        // Health check endpoint - no auth required
        .route("/health", get(health_check))
        .merge(
            Router::new()
                .route("/queues", post(create_queue))
                // Single message routes with 1MB limit
                .route(
                    "/queues/{name}/tasks",
                    post(enqueue)
                        .layer(DefaultBodyLimit::max(SINGLE_MESSAGE_LIMIT))
                        .get(dequeue),
                )
                // Batch routes with 10MB limit
                .route(
                    "/queues/{name}/tasks/batch",
                    post(queue_batch)
                        .layer(DefaultBodyLimit::max(BATCH_MESSAGE_LIMIT))
                        .get(dequeue_batch),
                )
                .route("/queues/{name}/tasks/ack/batch", post(ack_batch))
                .route("/queues/{name}/tasks/{task_id}/ack", post(ack_task))
                .route("/queues/{name}/tasks/{task_id}/nack", post(nack_task))
                .route(
                    "/queues/{name}/tasks/{task_id}/heartbeat",
                    post(heartbeat_task),
                )
                .route("/stats", get(get_stats))
                .route("/raft/vote", post(raft_vote))
                .route("/raft/append", post(raft_append))
                .route("/raft/snapshot", post(raft_snapshot))
                .route("/raft/init", post(raft_init))
                .route("/raft/add-learner", post(raft_add_learner))
                .route("/raft/change-membership", post(raft_change_membership))
                .route("/raft/metrics", get(raft_metrics))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
        .with_state(state)
}
