use axum::{
    Router,
    body::Bytes,
    extract::{Json, Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
pub mod storage;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use storage::Storage;
use storage::memory::InMemoryStorage;

#[derive(Clone)]
struct AppState {
    storage: Arc<InMemoryStorage>,
    password: String,
}

#[derive(Deserialize)]
struct CreateQueuePayload {
    name: String,
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let password_header = req.headers().get("x-oxideq-password");

    match password_header {
        Some(header) if header == state.password.as_str() => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn create_queue(
    State(state): State<AppState>,
    Json(payload): Json<CreateQueuePayload>,
) -> impl IntoResponse {
    match state.storage.create_queue(payload.name).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn enqueue(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
    payload: Bytes,
) -> impl IntoResponse {
    let item = payload.to_vec();
    match state.storage.queue(&queue_name, item).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn dequeue(
    State(state): State<AppState>,
    Path(queue_name): Path<String>,
) -> impl IntoResponse {
    match state.storage.dequeue(&queue_name).await {
        Ok(Some(task)) => {
            let message = String::from_utf8(task.data).unwrap_or_default();
            let mut response = message.into_response();
            if let Ok(val) = HeaderValue::from_str(&task.id) {
                response.headers_mut().insert("x-oxideq-task-id", val);
            }
            response
        }
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn ack_task(
    State(state): State<AppState>,
    Path((_queue_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.storage.ack(&task_id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn nack_task(
    State(state): State<AppState>,
    Path((_queue_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.storage.nack(&task_id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn get_stats(State(state): State<AppState>) -> impl IntoResponse {
    match state.storage.get_stats().await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub fn app(password: String, cleanup_interval: u64, task_timeout: u64) -> Router {
    let storage = Arc::new(InMemoryStorage::new());
    let state = AppState {
        storage: storage.clone(),
        password,
    };

    // Spawn cleanup task
    let storage_clone = storage.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval));
        loop {
            interval.tick().await;
            if let Err(e) = storage_clone.cleanup_expired_tasks(task_timeout).await {
                eprintln!("Cleanup error: {}", e);
            }
        }
    });

    Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .merge(
            Router::new()
                .route("/queues", post(create_queue))
                .route("/queues/{name}/tasks", post(enqueue).get(dequeue))
                .route("/queues/{name}/tasks/{task_id}/ack", post(ack_task))
                .route("/queues/{name}/tasks/{task_id}/nack", post(nack_task))
                .route("/stats", get(get_stats))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
        .with_state(state)
}
