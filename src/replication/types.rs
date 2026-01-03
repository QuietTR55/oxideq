use crate::storage::InFlightTask;
use openraft::Raft;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub type NodeId = u64;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ClientRequest,
        R = ClientResponse,
        NodeId = NodeId,
        Node = openraft::BasicNode,
        Entry = openraft::Entry<TypeConfig>,
        SnapshotData = Cursor<Vec<u8>>
);

pub type OxideRaft = Raft<TypeConfig>;

/// Commands that modify the state machine
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientRequest {
    CreateQueue {
        name: String,
    },
    Enqueue {
        queue: String,
        data: Vec<u8>,
    },
    /// Dequeue removes an item from queue and adds to in-flight, so it is a state change.
    Dequeue {
        queue: String,
    },
    Ack {
        task_id: String,
    },
    Nack {
        task_id: String,
    },
    ExtendVisibility {
        task_id: String,
        duration_seconds: u64,
    },

    // Batch Operations
    EnqueueBatch {
        queue: String,
        data: Vec<Vec<u8>>,
    },
    DequeueBatch {
        queue: String,
        count: usize,
    },
    AckBatch {
        task_ids: Vec<String>,
    },
}

/// Responses from the state machine
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientResponse {
    Ok,
    Task(Option<InFlightTask>),
    Tasks(Vec<InFlightTask>),
    Err(String),
}
