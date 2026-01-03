use crate::replication::types::*;
use crate::storage::Storage;
use crate::storage::memory::InMemoryStorage;
use openraft::storage::{RaftStorage, Snapshot};
use openraft::{
    AnyError, Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, RaftLogReader, SnapshotMeta,
    StorageError, StorageIOError, Vote,
};
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug)]
pub struct CombinedStorage {
    db: sled::Db,
    pub state_machine: Arc<InMemoryStorage>,
    pub last_applied_log: Arc<RwLock<Option<LogId<NodeId>>>>,
}

impl CombinedStorage {
    pub async fn new(
        state_machine: Arc<InMemoryStorage>,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        Ok(Self {
            db,
            state_machine,
            last_applied_log: Arc::new(RwLock::new(None)),
        })
    }
}

impl RaftLogReader<TypeConfig> for CombinedStorage {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<NodeId>> {
        let logs_tree = self.db.open_tree("logs").expect("failed to open logs tree");
        let start = match range.start_bound() {
            std::ops::Bound::Included(i) => *i,
            std::ops::Bound::Excluded(i) => *i + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end_bound = range.end_bound();

        let mut entries = Vec::new();

        for result in logs_tree.range(start.to_be_bytes()..) {
            let (key, value) = result.expect("sled error");
            let index = u64::from_be_bytes(key.as_ref().try_into().unwrap());

            match end_bound {
                std::ops::Bound::Included(e) if index > *e => break,
                std::ops::Bound::Excluded(e) if index >= *e => break,
                _ => {}
            }

            let entry: Entry<TypeConfig> =
                serde_json::from_slice(&value).expect("deserialize error");
            entries.push(entry);
        }

        Ok(entries)
    }
}

impl RaftStorage<TypeConfig> for CombinedStorage {
    type LogReader = Self;
    type SnapshotBuilder = Self;

    async fn get_log_state(
        &mut self,
    ) -> Result<openraft::storage::LogState<TypeConfig>, StorageError<NodeId>> {
        let logs_tree = self.db.open_tree("logs").expect("failed to open logs tree");
        let last = logs_tree.last().expect("sled error");

        let last_log_id = if let Some((_, value)) = last {
            let entry: Entry<TypeConfig> =
                serde_json::from_slice(&value).expect("deserialize error");
            Some(entry.log_id)
        } else {
            None
        };

        Ok(openraft::storage::LogState {
            last_purged_log_id: None,
            last_log_id,
        })
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        let store_tree = self
            .db
            .open_tree("store")
            .expect("failed to open store tree");
        let val = serde_json::to_vec(vote).unwrap();
        store_tree.insert("vote", val).expect("sled error");
        self.db.flush().expect("flush error");
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        let store_tree = self
            .db
            .open_tree("store")
            .expect("failed to open store tree");
        if let Some(val) = store_tree.get("vote").expect("sled error") {
            let vote = serde_json::from_slice(&val).expect("deserialize error");
            Ok(Some(vote))
        } else {
            Ok(None)
        }
    }

    async fn append_to_log<I>(&mut self, entries: I) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>>,
    {
        let logs_tree = self.db.open_tree("logs").expect("failed to open logs tree");
        for entry in entries {
            let key = entry.log_id.index.to_be_bytes();
            let value = serde_json::to_vec(&entry).unwrap();
            logs_tree.insert(key, value).expect("sled error");
        }
        self.db.flush().expect("flush error");
        Ok(())
    }

    async fn delete_conflict_logs_since(
        &mut self,
        log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        let logs_tree = self.db.open_tree("logs").expect("failed to open logs tree");
        let start = log_id.index.to_be_bytes();

        let mut batch = sled::Batch::default();
        for result in logs_tree.range(start..) {
            let (key, _) = result.expect("sled error");
            batch.remove(key);
        }
        logs_tree.apply_batch(batch).expect("sled error");
        self.db.flush().expect("flush error");
        Ok(())
    }

    async fn purge_logs_upto(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let logs_tree = self.db.open_tree("logs").expect("failed to open logs tree");

        let mut batch = sled::Batch::default();
        for result in logs_tree.range(..=log_id.index.to_be_bytes()) {
            let (key, _) = result.expect("sled error");
            batch.remove(key);
        }
        logs_tree.apply_batch(batch).expect("sled error");

        // Update last_purged
        let store_tree = self
            .db
            .open_tree("store")
            .expect("failed to open store tree");
        let val = serde_json::to_vec(&log_id).unwrap();
        store_tree
            .insert("last_purged_log_id", val)
            .expect("sled error");

        self.db.flush().expect("flush error");
        Ok(())
    }

    async fn last_applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<NodeId>>,
            openraft::StoredMembership<NodeId, openraft::BasicNode>,
        ),
        StorageError<NodeId>,
    > {
        let log_id = *self.last_applied_log.read().await;
        Ok((
            log_id,
            openraft::StoredMembership::new(log_id, openraft::Membership::default()),
        ))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<TypeConfig>],
    ) -> Result<Vec<ClientResponse>, StorageError<NodeId>> {
        let mut responses = Vec::new();

        for entry in entries {
            match &entry.payload {
                EntryPayload::Blank => responses.push(ClientResponse::Ok),
                EntryPayload::Normal(req) => {
                    let res = match req {
                        ClientRequest::CreateQueue { name } => {
                            match self.state_machine.create_queue(name.clone()).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::Enqueue { queue, data } => {
                            match self.state_machine.queue(queue, data.clone()).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::Dequeue { queue } => {
                            match self.state_machine.dequeue(queue).await {
                                Ok(task) => ClientResponse::Task(task),
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::Ack { task_id } => {
                            match self.state_machine.ack(task_id).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::Nack { task_id } => {
                            match self.state_machine.nack(task_id).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::ExtendVisibility {
                            task_id,
                            duration_seconds,
                        } => {
                            match self
                                .state_machine
                                .extend_visibility(task_id, *duration_seconds)
                                .await
                            {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::EnqueueBatch { queue, data } => {
                            match self.state_machine.queue_batch(queue, data.clone()).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::DequeueBatch { queue, count } => {
                            match self.state_machine.dequeue_batch(queue, *count).await {
                                Ok(tasks) => ClientResponse::Tasks(tasks),
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                        ClientRequest::AckBatch { task_ids } => {
                            match self.state_machine.ack_batch(task_ids.clone()).await {
                                Ok(_) => ClientResponse::Ok,
                                Err(e) => ClientResponse::Err(e),
                            }
                        }
                    };
                    responses.push(res);
                }
                EntryPayload::Membership(_) => responses.push(ClientResponse::Ok),
            }

            let mut guard = self.last_applied_log.write().await;
            *guard = Some(entry.log_id);
        }
        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, openraft::BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        let data = snapshot.into_inner();
        let (queues, in_flight) = serde_json::from_slice(&data).map_err(|e| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::Snapshot(Some(meta.signature())),
                ErrorVerb::Read,
                AnyError::new(&e),
            ),
        })?;

        self.state_machine.restore_from_snapshot(queues, in_flight);

        let mut guard = self.last_applied_log.write().await;
        *guard = meta.last_log_id;

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        Ok(None)
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }
}

impl openraft::storage::RaftSnapshotBuilder<TypeConfig> for CombinedStorage {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        let (queues, in_flight) = self.state_machine.get_snapshot_data();
        let data = serde_json::to_vec(&(&queues, &in_flight)).map_err(|e| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::Snapshot(None),
                ErrorVerb::Write,
                AnyError::new(&e),
            ),
        })?;

        let last_log = *self.last_applied_log.read().await;

        let meta = SnapshotMeta {
            last_log_id: last_log,
            last_membership: openraft::StoredMembership::new(
                last_log,
                openraft::Membership::default(),
            ),
            snapshot_id: format!("snap-{}", last_log.map(|l| l.index).unwrap_or(0)),
        };

        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(data)),
        })
    }
}
