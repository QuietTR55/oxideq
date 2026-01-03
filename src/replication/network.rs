use crate::replication::types::*;
use openraft::error::{InstallSnapshotError, NetworkError, RPCError, RaftError};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{BasicNode, RaftNetwork, RaftNetworkFactory};
use reqwest::Client;
use std::sync::Arc;

/// Network factory for Raft communication.
/// Uses a shared HTTP client for connection pooling.
pub struct Network {
    client: Arc<Client>,
}

impl Network {
    pub fn new() -> Self {
        // Create a shared client with connection pooling
        let client = Client::builder()
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client: Arc::new(client),
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NetworkConnection {
    client: Arc<Client>,
    #[allow(dead_code)]
    target: NodeId,
    target_node: BasicNode,
}

impl RaftNetworkFactory<TypeConfig> for Network {
    type Network = NetworkConnection;

    async fn new_client(&mut self, target: NodeId, node: &BasicNode) -> Self::Network {
        NetworkConnection {
            client: Arc::clone(&self.client),
            target,
            target_node: node.clone(),
        }
    }
}

impl RaftNetwork<TypeConfig> for NetworkConnection {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let url = format!("http://{}/raft/append", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        if !resp.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Error response: {}", resp.status()),
            ))));
        }

        let res: AppendEntriesResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        Ok(res)
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, BasicNode, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let url = format!("http://{}/raft/snapshot", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        if !resp.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Error response: {}", resp.status()),
            ))));
        }

        let res: InstallSnapshotResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        Ok(res)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let url = format!("http://{}/raft/vote", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        if !resp.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Error response: {}", resp.status()),
            ))));
        }

        let res: VoteResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        Ok(res)
    }
}
