use dotenvy::dotenv;
use oxideq::app;
use oxideq::replication::combined_storage::CombinedStorage;
use oxideq::replication::network::Network;
use oxideq::replication::types::{NodeId, OxideRaft};
use oxideq::storage::memory::InMemoryStorage;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    dotenv().ok();
    println!("Hello, world!");

    let password = env::var("OXIDEQ_PASSWORD").expect("OXIDEQ_PASSWORD must be set");

    let cleanup_interval = env::var("OXIDEQ_CLEANUP_INTERVAL")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .expect("OXIDEQ_CLEANUP_INTERVAL must be a number");

    let task_timeout = env::var("OXIDEQ_TASK_TIMEOUT")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .expect("OXIDEQ_TASK_TIMEOUT must be a number");

    let heartbeat_duration = env::var("OXIDEQ_HEARTBEAT_DURATION")
        .unwrap_or_else(|_| "60".to_string())
        .parse()
        .expect("OXIDEQ_HEARTBEAT_DURATION must be a number");

    let port_str = env::var("OXIDEQ_PORT").unwrap_or_else(|_| "8540".to_string());
    let port: u16 = port_str.parse().expect("OXIDEQ_PORT must be a number");
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // Cluster Mode detection
    let cluster_mode = env::var("OXIDEQ_CLUSTER_MODE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let storage: Arc<InMemoryStorage>;
    let raft: Option<OxideRaft>;

    if cluster_mode {
        println!("Starting in CLUSTER mode");
        let node_id_str =
            env::var("OXIDEQ_NODE_ID").expect("OXIDEQ_NODE_ID required in cluster mode");
        let node_id: NodeId = node_id_str.parse().expect("Invalid Node ID");

        // Note: OXIDEQ_RAFT_ADDR is required but nodes get their addresses from cluster config
        // This validates the env var is set for cluster mode documentation purposes
        let _ = env::var("OXIDEQ_RAFT_ADDR").expect("OXIDEQ_RAFT_ADDR required in cluster mode");

        let wal_path =
            env::var("OXIDEQ_WAL_PATH").unwrap_or_else(|_| format!("data/node_{}", node_id));
        std::fs::create_dir_all(&wal_path).unwrap();

        let state_machine = Arc::new(InMemoryStorage::new());
        let combined_storage =
            CombinedStorage::new(state_machine.clone(), PathBuf::from(&wal_path))
                .await
                .unwrap();

        let config = openraft::Config {
            heartbeat_interval: 500,
            election_timeout_min: 1500,
            election_timeout_max: 3000,
            ..Default::default()
        };

        let network = Network::new();

        // Wrap CombinedStorage in Adaptor for v2 API compatibility
        // Assuming Adaptor::new returns (LogStore, StateMachine) based on error messages
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

        storage = state_machine;
        raft = Some(raft_instance);
    } else {
        println!("Starting in STANDALONE mode");
        let wal_path = env::var("OXIDEQ_WAL_PATH").ok();
        storage = if let Some(path) = wal_path {
            Arc::new(InMemoryStorage::new_persistent(&path).await)
        } else {
            Arc::new(InMemoryStorage::new())
        };
        raft = None;
    };

    let app = app(
        storage,
        password,
        cleanup_interval,
        task_timeout,
        heartbeat_duration,
        raft,
    )
    .await;

    println!("Server running on http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
