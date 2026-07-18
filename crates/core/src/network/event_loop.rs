//! Core 事件循环
//!
//! 完全平台无关：消费 [`Endpoint::subscribe`](swarmdrop_net::Endpoint::subscribe) 的
//! [`NetEvent`] 边沿事件，喂给 devices / presence / infra，并把高层事件推给 host；
//! 内核状态（地址 / NAT）经 watch 变更驱动 `NetworkStatusChanged`。
//!
//! 业务入站（pairing / transfer 请求）**不在此处**——它们由 Router 的 RPC / 数据面
//! handler 直接吸收（见 `runtime::start_node`）。本循环只做状态镜像与设备/网络事件转发。

use std::sync::Arc;

use swarmdrop_net::{Events, InfraRoles, NetEvent, NodeAddr, Router};
use tracing::{info, warn};

use super::SharedNetRefs;
use super::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
use crate::device::OsInfo;
use crate::device_manager::DeviceFilter;
use crate::host::{CoreEvent, EventBus};
use crate::protocol::IDENTIFY_PROTOCOL;
use crate::transfer::incoming::IncomingTransferRuntime;

/// 处理一个 [`NetEvent`] —— 设备/presence/infra 折叠 + 高层事件转发。
///
/// 不含传输侧的断连 hook（交给 [`run_event_loop`]，它有 `IncomingTransferRuntime`
/// 约束）；mobile-core 的 `NetManager<()>` 仅需处理网络事件时可直接调本函数。
pub async fn handle_core_node_event<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NetEvent,
    event_bus: &dyn EventBus,
) where
    TTransfer: Send + Sync + 'static,
{
    // 让各子系统自己折叠状态
    shared.devices.handle_event(event);
    shared.presence.handle_event(event);
    shared.infra.handle_event(event);
    maybe_register_lan_helper(shared, event, event_bus).await;
    let refreshed_paired_device = refresh_paired_device_from_identify(shared, event);

    match event {
        NetEvent::PeerConnected { node, .. } => {
            if let Ok(mut candidates) = shared.candidates.write() {
                candidates.mark_connected(*node);
            }
            publish_devices_and_status(shared, event_bus).await;
        }
        NetEvent::PeerDisconnected { .. } => {
            publish_devices_and_status(shared, event_bus).await;
        }
        NetEvent::RelayReservationAccepted { relay, .. } => {
            if let Ok(mut candidates) = shared.candidates.write() {
                candidates.mark_relay_ready(*relay);
            }
            publish_network_status(shared, event_bus).await;
        }
        NetEvent::RelayReservationLost { relay } => {
            // 重建由 InfraSupervisor 负责（顶部 handle_event 已折叠）；这里只更新状态视图
            if let Ok(mut candidates) = shared.candidates.write() {
                candidates.mark_failed(*relay);
            }
            publish_network_status(shared, event_bus).await;
        }
        NetEvent::PeerIdentified { .. }
        | NetEvent::Discovered { .. }
        | NetEvent::PingSuccess { .. }
        | NetEvent::PathChanged { .. } => {
            // TODO(ui-rewrite): PingSuccess 触发全量 publish 待 UI 重写时按 rtt 阈值/去抖收敛
            publish_devices_and_status(shared, event_bus).await;
        }
        NetEvent::PingFailure { node, error } => {
            // 已配对 peer 的死对端判定在 presence supervisor（顶部 handle_event）完成
            tracing::debug!("Ping 失败 {}: {}", node, error);
        }
    }
    if let Some(device) = refreshed_paired_device {
        // 已配对设备的名称/系统信息来自对端 identify；由 host 消费此事件后写入持久化存储。
        let _ = event_bus
            .publish(CoreEvent::PairedDeviceAdded { device })
            .await;
    }
}

fn refresh_paired_device_from_identify<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NetEvent,
) -> Option<crate::device::PairedDeviceInfo> {
    let NetEvent::PeerIdentified { node, agent, .. } = event else {
        return None;
    };

    OsInfo::from_agent_version(agent)
        .and_then(|os_info| shared.pairing.refresh_paired_device_os_info(node, os_info))
}

async fn maybe_register_lan_helper<TTransfer>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NetEvent,
    event_bus: &dyn EventBus,
) where
    TTransfer: Send + Sync + 'static,
{
    let NetEvent::PeerIdentified {
        node,
        agent,
        protocol,
        addrs,
        ..
    } = event
    else {
        return;
    };

    if !shared.network_config.auto_discover_lan_helpers
        || protocol != IDENTIFY_PROTOCOL
        || !OsInfo::is_swarmdrop_agent(agent)
    {
        return;
    }
    let Some(os_info) = OsInfo::from_agent_version(agent) else {
        return;
    };
    if !os_info.has_capability(OsInfo::LAN_HELPER_CAPABILITY) {
        return;
    }

    let addrs = usable_lan_candidate_addrs(addrs);
    if addrs.is_empty() {
        publish_network_status(shared, event_bus).await;
        return;
    }

    let changed = shared
        .candidates
        .write()
        .map(|mut candidates| {
            candidates.upsert(
                *node,
                addrs.clone(),
                BootstrapCandidateSource::MdnsLanHelper,
                CandidateRoles::kad_and_relay(),
                CandidateScope::Lan,
            )
        })
        .unwrap_or(false);

    // changed 只是"地址/来源有更新"的即时接线路径（避免每次 identify 重复发命令）。
    // 候选进表后，连接与 reservation 的持续维持由 InfraSupervisor 收敛兜底——
    // helper 重启/挂起恢复（changed=false）的重建不再依赖这里。
    if changed {
        let endpoint = shared.endpoint.clone();
        let candidates = shared.candidates.clone();
        let node = *node;
        n0_future::task::spawn(async move {
            if let Err(err) = endpoint
                .add_infrastructure_peer(
                    NodeAddr::with_addrs(node, addrs),
                    InfraRoles {
                        kad_server: true,
                        relay: true,
                    },
                )
                .await
            {
                warn!("注册 LAN Helper 候选失败 {}: {}", node, err);
                if let Ok(mut candidates) = candidates.write() {
                    candidates.mark_failed(node);
                }
                return;
            }
            if let Some(dht) = endpoint.dht()
                && let Err(err) = dht.bootstrap().await
            {
                warn!("LAN Helper 候选触发 bootstrap 失败 {}: {}", node, err);
            }
        });
    }

    publish_network_status(shared, event_bus).await;
}

fn usable_lan_candidate_addrs(addrs: &[swarmdrop_net::Addr]) -> Vec<swarmdrop_net::Addr> {
    addrs
        .iter()
        .filter(|addr| addr.is_private_lan())
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

/// 完整事件循环：消费 [`NetEvent`] + 拉起 presence / infra 后台收敛 + watch 驱动状态。
pub async fn run_event_loop<TTransfer>(
    mut events: Events,
    shared: SharedNetRefs<TTransfer>,
    event_bus: Arc<dyn EventBus>,
) where
    TTransfer: IncomingTransferRuntime + Send + Sync + 'static,
{
    // presence / infra 后台收敛任务随事件循环拉起，随 NetManager 的
    // CancellationToken 退出。host 无需任何 presence/infra 调用。
    n0_future::task::spawn(
        shared
            .presence
            .clone()
            .run(shared.clone(), event_bus.clone()),
    );
    n0_future::task::spawn(shared.infra.clone().run(shared.clone()));

    // 内核状态（本机地址 / NAT）无对应边沿事件，经 watch 变更驱动状态刷新。
    let mut addrs_watcher = shared.endpoint.watch_addrs();
    let mut nat_watcher = shared.endpoint.watch_nat();

    loop {
        tokio::select! {
            // 节点停止：退出循环使本函数持有的 shared 全部 drop，配合
            // NetManager::shutdown 关闭 Endpoint（drop Swarm → 断开全部连接）。
            _ = shared.cancel_token.cancelled() => break,
            maybe = events.recv() => match maybe {
                Some(event) => {
                    handle_core_node_event(&shared, &event, event_bus.as_ref()).await;
                    // 对端断连：把该 peer 当前 active 传输转为 recoverable suspended(Interrupted)。
                    // 发送端会话本就 idle、靠此 hook 感知断连；接收端 data stream 读取会因连接丢失失败，
                    // 但 handle_peer_disconnected 先取消会话（cancel 优先于 error）避免误判 failed。
                    if let NetEvent::PeerDisconnected { node } = &event {
                        shared.transfer.handle_peer_disconnected(*node).await;
                    }
                }
                None => break,
            },
            Some(_) = addrs_watcher.updated() => {
                publish_network_status(&shared, event_bus.as_ref()).await;
            }
            Some(_) = nat_watcher.updated() => {
                publish_network_status(&shared, event_bus.as_ref()).await;
            }
        }
    }
    info!("事件循环退出");
}

/// 把 [`run_event_loop`] spawn 到 tokio task，并让 `router` 与之同生命周期。
///
/// `router` 由 spawn 的 task 持有——事件循环退出（`NetManager::shutdown` 关闭
/// Endpoint）时一并 drop，入站流路由随之停止；host 无需单独的 state 槽保活。桌面壳
/// （`src-tauri`）与移动端（`mobile-core`）共用同一装配，避免保活语义漂移。
pub fn spawn_event_loop<TTransfer>(
    events: Events,
    shared: SharedNetRefs<TTransfer>,
    event_bus: Arc<dyn EventBus>,
    router: Router,
) where
    TTransfer: IncomingTransferRuntime + Send + Sync + 'static,
{
    n0_future::task::spawn(async move {
        run_event_loop(events, shared, event_bus).await;
        drop(router);
    });
}

#[cfg(test)]
mod tests {
    use super::usable_lan_candidate_addrs;
    use swarmdrop_net::Addr;

    #[test]
    fn lan_helper_candidates_filter_unusable_addresses() {
        let addrs: Vec<Addr> = [
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
