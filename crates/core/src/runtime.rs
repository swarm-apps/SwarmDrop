//! Core runtime 入口。

use swarm_p2p_core::libp2p::{identity::Keypair, PeerId};
use swarm_p2p_core::EventReceiver;

use crate::device::{OsInfo, PairedDeviceInfo};
use crate::error::{AppError, AppResult};
use crate::network::config::create_node_config;
use crate::network::{NetManager, TransferRuntime};
use crate::protocol::{AppNetClient, AppRequest, AppResponse};

/// 已启动的 core 节点。
pub struct StartedNode<TTransfer>
where
    TTransfer: TransferRuntime,
{
    pub manager: NetManager<TTransfer>,
    pub receiver: EventReceiver<AppRequest>,
}

/// 启动 P2P 节点并创建 core 网络管理器。
pub fn start_node<TTransfer, F>(
    keypair: Keypair,
    paired_devices: Vec<PairedDeviceInfo>,
    custom_bootstrap_nodes: Vec<String>,
    create_transfer: F,
) -> AppResult<StartedNode<TTransfer>>
where
    TTransfer: TransferRuntime,
    F: FnOnce(AppNetClient) -> TTransfer,
{
    let agent_version = OsInfo::default().to_agent_version();
    let config = create_node_config(agent_version, &custom_bootstrap_nodes);

    let peer_id = PeerId::from_public_key(&keypair.public());
    let (client, receiver) = swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, config)
        .map_err(|e| AppError::Network(e.to_string()))?;

    let transfer = create_transfer(client.clone());
    let manager = NetManager::new(client, peer_id, paired_devices, transfer);

    Ok(StartedNode { manager, receiver })
}
