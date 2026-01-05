use dotenvy::dotenv;
use oxideq::app;
use oxideq::replication::combined_storage::CombinedStorage;
use oxideq::replication::network::Network;
use oxideq::replication::types::{NodeId, OxideRaft};
use oxideq::server::run_server;
use oxideq::shutdown::shutdown_signal;
use oxideq::storage::memory::InMemoryStorage;
use oxideq::telemetry::init_logging;
use oxideq::tls::TlsConfig;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    dotenv().ok();

    // Initialize logging first - required for all subsequent tracing
    init_logging();

    tracing::info!("OxideQ starting");

    let password = env::var("OXIDEQ_PASSWORD").expect("OXIDEQ_PASSWORD must be set");

    let cleanup_interval: u64 = env::var("OXIDEQ_CLEANUP_INTERVAL")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .expect("OXIDEQ_CLEANUP_INTERVAL must be a number");

    let task_timeout: u64 = env::var("OXIDEQ_TASK_TIMEOUT")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .expect("OXIDEQ_TASK_TIMEOUT must be a number");

    let heartbeat_duration: u64 = env::var("OXIDEQ_HEARTBEAT_DURATION")
        .unwrap_or_else(|_| "60".to_string())
        .parse()
        .expect("OXIDEQ_HEARTBEAT_DURATION must be a number");

    let port_str = env::var("OXIDEQ_PORT").unwrap_or_else(|_| "8540".to_string());
    let port: u16 = port_str.parse().expect("OXIDEQ_PORT must be a number");
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // Load TLS configuration (optional)
    let tls_config = TlsConfig::from_env().expect("Failed to load TLS configuration");
    if tls_config.is_some() {
        tracing::info!("TLS enabled");
    }

    // Cluster Mode detection
    let cluster_mode = env::var("OXIDEQ_CLUSTER_MODE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let storage: Arc<InMemoryStorage>;
    let raft: Option<OxideRaft>;

    if cluster_mode {
        tracing::info!("Starting in CLUSTER mode");
        let node_id_str =
            env::var("OXIDEQ_NODE_ID").expect("OXIDEQ_NODE_ID required in cluster mode");
        let node_id: NodeId = node_id_str.parse().expect("Invalid Node ID");

        // Note: OXIDEQ_RAFT_ADDR is required but nodes get their addresses from cluster config
        // This validates the env var is set for cluster mode documentation purposes
        let _ = env::var("OXIDEQ_RAFT_ADDR").expect("OXIDEQ_RAFT_ADDR required in cluster mode");

        let wal_path =
            env::var("OXIDEQ_WAL_PATH").unwrap_or_else(|_| format!("data/node_{}", node_id));
        std::fs::create_dir_all(&wal_path).expect("Failed to create WAL directory");

        let state_machine = Arc::new(InMemoryStorage::new());
        let combined_storage =
            CombinedStorage::new(state_machine.clone(), PathBuf::from(&wal_path))
                .await
                .expect("Failed to create combined storage");

        let config = openraft::Config {
            heartbeat_interval: 500,
            election_timeout_min: 1500,
            election_timeout_max: 3000,
            ..Default::default()
        };

        let network = Network::new();

        // Wrap CombinedStorage in Adaptor for v2 API compatibility
        let (log_store, state_machine_store) = openraft::storage::Adaptor::new(combined_storage);

        let raft_instance = openraft::Raft::new(
            node_id,
            Arc::new(config),
            network,
            log_store,
            state_machine_store,
        )
        .await
        .expect("Failed to create Raft");

        tracing::info!(node_id = node_id, wal_path = %wal_path, "Raft node initialized");

        storage = state_machine;
        raft = Some(raft_instance);
    } else {
        tracing::info!("Starting in STANDALONE mode");
        let wal_path = env::var("OXIDEQ_WAL_PATH").ok();
        storage = if let Some(path) = &wal_path {
            tracing::info!(wal_path = %path, "WAL persistence enabled");
            Arc::new(InMemoryStorage::new_persistent(path).await)
        } else {
            tracing::warn!("Running without persistence - data will be lost on restart");
            Arc::new(InMemoryStorage::new())
        };
        raft = None;
    };

    // Create shutdown token for cleanup task coordination
    let shutdown_token = CancellationToken::new();

    let router = app(
        storage,
        password,
        cleanup_interval,
        task_timeout,
        heartbeat_duration,
        raft,
        shutdown_token.clone(),
    )
    .await;

    // Run server with graceful shutdown
    // When shutdown signal is received, cancel the cleanup task first
    let token_clone = shutdown_token.clone();
    let shutdown_future = async move {
        shutdown_signal().await;
        token_clone.cancel();
        tracing::info!("Shutdown signal received, draining connections...");
    };

    if let Err(e) = run_server(router, addr, tls_config, shutdown_future).await {
        tracing::error!(error = %e, "Server error");
        std::process::exit(1);
    }
}
