//! Core 事件循环
//!
//! 完全平台无关：消费 [`swarm_p2p_core::NodeEvent`]，更新 [`SharedNetRefs`] 中的
//! 网络/设备状态，通过 [`EventBus`] 把高层事件推送给 host。
//!
//! - 网络/设备/配对事件直接处理
//! - 传输事件交给 [`IncomingTransferRuntime`] 实现
//! - host 端（Tauri / RN）只需提供 EventBus + IncomingTransferRuntime + Notifier
//!
//! 调用方一般用 [`run_event_loop`]（自带消费循环）；
//! 仅需处理网络事件时也可单独调 [`handle_core_node_event`]。

use std::sync::Arc;

use swarm_p2p_core::libp2p::{Multiaddr, PeerId, multiaddr::Protocol};
use swarm_p2p_core::{EventReceiver, InfrastructureRoles, NodeEvent};
use tracing::{info, warn};

use super::SharedNetRefs;
use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use crate::device::{ConnectionType, OsInfo};
use crate::device_manager::DeviceFilter;
use crate::error::AppResult;
use crate::host::{CoreEvent, EventBus, Notifier};
use crate::protocol::AppRequest;
use crate::transfer::incoming::{
    IncomingTransferDisposition, IncomingTransferRuntime, handle_incoming_transfer_request,
};

/// 处理一个 NodeEvent —— 网络/设备/配对事件部分
///
/// 不处理 `InboundRequest::Transfer`（交给 caller 用 `IncomingTransferRuntime`）。
/// mobile-core 当前 transfer 还是占位时，可只调用此函数即可。
pub async fn handle_core_node_event<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NodeEvent<AppRequest>,
    event_bus: &dyn EventBus,
) -> AppResult<()> {
    // 让 DeviceManager 自己处理 PeerConnected/Disconnected/Identify/Ping/HolePunch/Discovered
    shared.devices.handle_event(event);
    // presence 状态机折叠（已配对 peer 的 Connected/Probing 转换 + 断连即重拨）
    shared.presence.handle_event(event);
    maybe_register_lan_helper(shared, event, event_bus).await;

    match event {
        NodeEvent::Listening { addr } => {
            if let Ok(mut listen) = shared.listen_addrs.write()
                && !listen.contains(addr)
            {
                listen.push(addr.clone());
            }
            publish_network_status(shared, event_bus).await;
        }
        NodeEvent::NatStatusChanged {
            status,
            public_addr,
        } => {
            if let Ok(mut nat) = shared.nat_status.write() {
                *nat = status.clone();
            }
            if let Ok(mut addr) = shared.public_addr.write() {
                addr.clone_from(public_addr);
            }
            publish_network_status(shared, event_bus).await;
        }
        NodeEvent::RelayReservationAccepted { relay_peer_id, .. } => {
            if let Ok(mut rp) = shared.relay_peers.write() {
                rp.insert(*relay_peer_id);
            }
            if let Ok(mut candidates) = shared.candidates.write() {
                candidates.mark_relay_ready(*relay_peer_id);
            }
            publish_network_status(shared, event_bus).await;
        }
        NodeEvent::LanHelperStatusChanged {
            relay_server_enabled,
            advertised_addrs,
        } => {
            if let Ok(mut enabled) = shared.relay_server_enabled.write() {
                *enabled = *relay_server_enabled;
            }
            if let Ok(mut addrs) = shared.lan_helper_advertised_addrs.write() {
                *addrs = advertised_addrs.clone();
            }
            publish_network_status(shared, event_bus).await;
        }
        NodeEvent::PeerConnected { .. }
        | NodeEvent::IdentifyReceived { .. }
        | NodeEvent::PeersDiscovered { .. }
        | NodeEvent::PingSuccess { .. }
        | NodeEvent::HolePunchSucceeded { .. }
        | NodeEvent::RelayServerReservationAccepted { .. }
        | NodeEvent::RelayServerReservationDenied { .. }
        | NodeEvent::RelayServerReservationClosed { .. }
        | NodeEvent::RelayServerCircuitAccepted { .. }
        | NodeEvent::RelayServerCircuitDenied { .. }
        | NodeEvent::RelayServerCircuitClosed { .. } => {
            if let NodeEvent::PeerConnected { peer_id } = event
                && let Ok(mut candidates) = shared.candidates.write()
            {
                candidates.mark_connected(*peer_id);
            }
            publish_devices_and_status(shared, event_bus).await;
        }
        NodeEvent::PeerDisconnected { peer_id } => {
            if let Ok(mut rp) = shared.relay_peers.write() {
                rp.remove(peer_id);
            }
            publish_devices_and_status(shared, event_bus).await;
        }
        NodeEvent::HolePunchFailed { peer_id, error } => {
            warn!("Hole punch failed with {}: {}", peer_id, error);
        }
        NodeEvent::PingFailure { peer_id, error } => {
            // 已配对 peer 的死对端判定在 presence supervisor（顶部 handle_event）完成
            tracing::debug!("Ping 失败 {}: {}", peer_id, error);
        }
        NodeEvent::InboundRequest {
            peer_id,
            pending_id,
            request: AppRequest::Pairing(pair_req),
        } => {
            shared
                .pairing
                .cache_inbound_request(*peer_id, *pending_id, pair_req);

            let _ = event_bus
                .publish(CoreEvent::PairingRequestReceived {
                    peer_id: *peer_id,
                    pending_id: *pending_id,
                    request: pair_req.clone(),
                })
                .await;
        }
        NodeEvent::InboundRequest {
            request: AppRequest::Transfer(_),
            ..
        } => {
            // 交给 caller 通过 IncomingTransferRuntime 处理
        }
    }
    Ok(())
}

async fn maybe_register_lan_helper<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NodeEvent<AppRequest>,
    event_bus: &dyn EventBus,
) {
    let NodeEvent::IdentifyReceived {
        peer_id,
        agent_version,
        protocol_version,
        listen_addrs,
        ..
    } = event
    else {
        return;
    };

    if !shared.network_config.auto_discover_lan_helpers
        || protocol_version != "/swarmdrop/1.0.0"
        || !OsInfo::is_swarmdrop_agent(agent_version)
    {
        return;
    }
    let Some(os_info) = OsInfo::from_agent_version(agent_version) else {
        return;
    };
    if !os_info.has_capability(OsInfo::LAN_HELPER_CAPABILITY) {
        return;
    }

    let addrs = usable_lan_candidate_addrs(listen_addrs);
    if addrs.is_empty() {
        publish_network_status(shared, event_bus).await;
        return;
    }

    let changed = shared
        .candidates
        .write()
        .map(|mut candidates| {
            candidates.upsert(
                *peer_id,
                addrs.clone(),
                BootstrapCandidateSource::MdnsLanHelper,
                CandidateRoles::kad_and_relay(),
                CandidateScope::Lan,
            )
        })
        .unwrap_or(false);

    if changed {
        let client = shared.client.clone();
        let candidates = shared.candidates.clone();
        let peer_id = *peer_id;
        tokio::spawn(async move {
            if let Err(err) = client
                .add_infrastructure_peer(peer_id, addrs, InfrastructureRoles::kad_and_relay())
                .await
            {
                warn!("注册 LAN Helper 候选失败 {}: {}", peer_id, err);
                if let Ok(mut candidates) = candidates.write() {
                    candidates.mark_failed(peer_id);
                }
                return;
            }
            if let Err(err) = client.bootstrap().await {
                warn!("LAN Helper 候选触发 bootstrap 失败 {}: {}", peer_id, err);
            }
        });
    }

    publish_network_status(shared, event_bus).await;
}

fn usable_lan_candidate_addrs(addrs: &[Multiaddr]) -> Vec<Multiaddr> {
    addrs
        .iter()
        .filter(|addr| {
            addr.iter().any(|protocol| match protocol {
                Protocol::Ip4(ip) => {
                    ip.is_private()
                        && !ip.is_loopback()
                        && !ip.is_link_local()
                        && !ip.is_unspecified()
                }
                Protocol::Ip6(ip) => {
                    (ip.segments()[0] & 0xfe00) == 0xfc00
                        && !ip.is_loopback()
                        && !ip.is_unspecified()
                }
                _ => false,
            })
        })
        .cloned()
        .collect()
}

pub(crate) async fn publish_devices_and_status<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event_bus: &dyn EventBus,
) {
    let devices = shared.devices.get_devices(DeviceFilter::All);
    let _ = event_bus
        .publish(CoreEvent::DevicesChanged { devices })
        .await;
    let status = shared.build_network_status();
    let _ = event_bus
        .publish(CoreEvent::NetworkStatusChanged { status })
        .await;
}

async fn publish_network_status<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event_bus: &dyn EventBus,
) {
    let status = shared.build_network_status();
    let _ = event_bus
        .publish(CoreEvent::NetworkStatusChanged { status })
        .await;
}

/// 完整事件循环：handle_core_node_event + IncomingTransferRuntime
///
/// `notifier` 用于"未聚焦时弹通知"（桌面端有意义；移动端可传 None）。
pub async fn run_event_loop<TTransfer>(
    mut receiver: EventReceiver<AppRequest>,
    shared: SharedNetRefs<TTransfer>,
    event_bus: Arc<dyn EventBus>,
    notifier: Option<Arc<dyn Notifier>>,
) where
    TTransfer: IncomingTransferRuntime + Send + Sync + 'static,
{
    // presence 后台任务随事件循环拉起（宣告/bootstrap/保活装载/状态机推进），
    // 随 NetManager 的 CancellationToken 退出。host 无需任何 presence 调用。
    tokio::spawn(
        shared
            .presence
            .clone()
            .run(shared.clone(), event_bus.clone()),
    );

    while let Some(event) = receiver.recv().await {
        if let Err(e) = handle_core_node_event(&shared, &event, event_bus.as_ref()).await {
            warn!("core 节点事件处理失败: {}", e);
        }

        // 对端断连：把该 peer 当前 active 传输转为 recoverable suspended(Interrupted)。
        // 发送端会话本就 idle、靠此 hook 感知断连；接收端 data channel 读取会因连接丢失失败，
        // 但 handle_peer_disconnected 先取消会话（cancel 优先于 error）避免误判 failed。
        if let NodeEvent::PeerDisconnected { peer_id } = &event {
            shared.transfer.handle_peer_disconnected(*peer_id).await;
        }

        // 通知：配对请求触发系统通知
        if let NodeEvent::InboundRequest {
            request: AppRequest::Pairing(req),
            ..
        } = &event
            && let Some(notifier) = notifier.as_ref()
        {
            let _ = notifier
                .notify_if_unfocused(crate::host::Notification::PairingRequest {
                    hostname: req.os_info.hostname.clone(),
                })
                .await;
        }

        // 处理传输请求
        if let NodeEvent::InboundRequest {
            peer_id,
            pending_id,
            request: AppRequest::Transfer(transfer_request),
        } = event
        {
            let is_offer = matches!(
                transfer_request,
                crate::protocol::TransferRequest::Offer { .. }
            );

            let paired_device = is_offer
                .then(|| shared.pairing.get_paired_device(&peer_id))
                .flatten();

            // Offer：先校验 pairing 状态，避免向未配对方泄露设备信息
            if is_offer && paired_device.is_none() {
                warn!("Rejecting transfer offer from unpaired peer: {}", peer_id);
                let response = crate::protocol::AppResponse::Transfer(
                    crate::protocol::TransferResponse::OfferResult {
                        accepted: false,
                        key: None,
                        reason: Some(crate::protocol::OfferRejectReason::NotPaired),
                    },
                );
                let _ = shared.client.send_response(pending_id, response).await;
                continue;
            }

            // Offer：用 paired_devices 中的真实设备名（hostname）覆盖 peer_id 短串
            let device_name_override = if is_offer {
                paired_device.as_ref().map(display_device_name)
            } else {
                None
            };
            let via_relay = is_offer && is_peer_connected_via_relay(&shared, &peer_id);

            let result = handle_incoming_transfer_request(
                &shared.client,
                shared.transfer.as_ref(),
                event_bus.as_ref(),
                peer_id,
                pending_id,
                paired_device,
                via_relay,
                transfer_request,
            )
            .await;

            match result {
                Ok(IncomingTransferDisposition::Handled) => {
                    // 已自动接收或已策略拒绝的 Offer 只进入活动与恢复，不弹确认通知。
                }
                Ok(IncomingTransferDisposition::OfferRequiresConfirmation) => {
                    if is_offer
                        && let (Some(notifier), Some(name)) =
                            (notifier.as_ref(), device_name_override)
                    {
                        let _ = notifier
                            .notify_if_unfocused(crate::host::Notification::IncomingTransfer {
                                device_name: name,
                            })
                            .await;
                    }
                }
                Ok(IncomingTransferDisposition::Unhandled(_)) => {
                    warn!("传输请求未被处理（可能为新协议变体）");
                }
                Err(e) => {
                    warn!("传输请求处理失败: {}", e);
                }
            }
        }
    }
    info!("事件循环退出");
}

fn is_peer_connected_via_relay<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    peer_id: &PeerId,
) -> bool {
    shared
        .devices
        .get_devices(DeviceFilter::All)
        .into_iter()
        .find(|device| &device.peer_id == peer_id)
        .is_some_and(|device| device.connection == Some(ConnectionType::Relay))
}

fn display_device_name(device: &crate::device::PairedDeviceInfo) -> String {
    device
        .os_info
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| device.os_info.hostname.clone())
}

#[cfg(test)]
mod tests {
    use super::usable_lan_candidate_addrs;
    use swarm_p2p_core::libp2p::Multiaddr;

    #[test]
    fn lan_helper_candidates_filter_unusable_addresses() {
        let addrs: Vec<Multiaddr> = [
            "/ip4/192.168.1.20/tcp/4001",
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/0.0.0.0/tcp/4001",
            "/ip4/8.8.8.8/tcp/4001",
            "/ip6/fd00::1/tcp/4001",
        ]
        .into_iter()
        .map(|addr| addr.parse().unwrap())
        .collect();

        let usable = usable_lan_candidate_addrs(&addrs);

        assert_eq!(usable.len(), 2);
        assert_eq!(usable[0].to_string(), "/ip4/192.168.1.20/tcp/4001");
        assert_eq!(usable[1].to_string(), "/ip6/fd00::1/tcp/4001");
    }
}
