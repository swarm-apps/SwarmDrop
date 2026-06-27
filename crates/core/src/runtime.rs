//! Core runtime 入口。

use swarm_p2p_core::EventReceiver;
use swarm_p2p_core::libp2p::{PeerId, identity::Keypair};

use crate::device::{OsInfo, PairedDeviceInfo};
use crate::error::{AppError, AppResult};
use crate::network::config::{NetworkRuntimeConfig, create_candidate_manager, create_node_config};
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
///
/// `device_name` 来自 host 持久化层（桌面端的 device_config.json / 移动端的 RN
/// settings），节点启动时塞入 `OsInfo.name`，通过 libp2p Identify 的
/// `agent_version` 字段广播给对端。Host 改名后需 stop + start 节点让新值上线。
pub fn start_node<TTransfer, F>(
    keypair: Keypair,
    device_name: Option<String>,
    paired_devices: Vec<PairedDeviceInfo>,
    network_config: NetworkRuntimeConfig,
    create_transfer: F,
) -> AppResult<StartedNode<TTransfer>>
where
    TTransfer: TransferRuntime,
    F: FnOnce(AppNetClient) -> TTransfer,
{
    let os_info = OsInfo {
        name: device_name,
        ..OsInfo::default()
    };
    let os_info = if network_config.provide_lan_helper {
        os_info.with_capability(OsInfo::LAN_HELPER_CAPABILITY)
    } else {
        os_info
    };
    let agent_version = os_info.to_agent_version();
    let config = create_node_config(agent_version, &network_config);
    let candidate_manager = create_candidate_manager(&network_config);

    let peer_id = PeerId::from_public_key(&keypair.public());
    // 第三个返回值是 data-channel 入站接收器（add-p2p-data-channel 能力）。
    // 本 change 仅提供能力、不接线文件传输；redesign-transfer-lifecycle 的
    // Phase B 会接管它。当前 config 未注册任何 data-channel 协议，drop 无副作用。
    let (client, receiver, _dc_receiver) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, config)
            .map_err(|e| AppError::Network(e.to_string()))?;

    let transfer = create_transfer(client.clone());
    let manager = NetManager::new(
        client,
        peer_id,
        paired_devices,
        transfer,
        network_config,
        candidate_manager,
    );

    Ok(StartedNode { manager, receiver })
}
