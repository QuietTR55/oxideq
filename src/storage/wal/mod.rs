use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct WalEntry {
    pub op: WalOp,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WalOp {
    CreateQueue(String),
    Enqueue {
        queue: String,
        data: Vec<u8>,
    },
    EnqueueBatch {
        queue: String,
        data: Vec<Vec<u8>>,
    },
    Dequeued {
        queue: String,
        task_id: String,
    },
    DequeuedBatch {
        queue: String,
        task_ids: Vec<String>,
    },
    Ack(String),
    AckBatch(Vec<String>),
    Nack(String),
    // Heartbeat?
    ExtendVisibility {
        task_id: String,
        duration: u64,
    },
}

pub mod manager;
