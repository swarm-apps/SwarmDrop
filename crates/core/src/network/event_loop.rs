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
use super::candidates::{BootstrapCandidateSource, CandidateRoles};
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

    // 候选表只存**意图**，不存观测。连接与 reservation 的事实分别由
    // `watch_conns` / `watch_relays` 承载，由 `build_infra_links` 现场合成——
    // 此前这几个分支往候选表回写健康位，而另外四条路径（用户撤销、内核注销、
    // 多条拨号失败）不回写，于是那个位会停在「已就绪」不动。
    match event {
        NetEvent::PeerConnected { .. } | NetEvent::PeerDisconnected { .. } => {
            publish_devices_and_status(shared, event_bus).await;
        }
        // relay 两个边沿**刻意不推**：内核在 emit 之前必然先写 `watch_relays`
        // （`actor.rs` 的 `set_relay_state` / `set_relay_failed` 都在 emit 前一行），
        // 而 `run_event_loop` 订阅了那条 watch —— 两边都推就是每次转换重建两遍全量
        // `NetworkStatus`（候选快照克隆 + 两张 watch 深拷贝，移动端还要再过一次 uniffi）。
        //
        // 顺带修掉一处周期性空推送：reservation 每轮续期都发 `Accepted{renewal:true}`，
        // 而 watch 侧 `send_if_modified` 判值相等**不通知**（那是刻意的静音）。这条分支
        // 把静音又拆了回来，于是每个 relay 每个续期周期都在推一份一模一样的状态。
        //
        // watch 只承载 relay 三态，`ever_active` 由顶部 `infra.handle_event` 折叠、
        // 两条轨的到达顺序不定——但 `deriveInfraLinkState` 先判 `relay == active`
        // 才看 `everActive`，两者唯一同时生效的组合（曾活跃过、现已失败）里
        // `ever_active` 早在更早的 `Accepted` 就置位了，读得到。
        NetEvent::RelayReservationAccepted { .. } | NetEvent::RelayReservationLost { .. } => {}
        // ping 是**周期性心跳**（`ping_interval` 30s/peer），不是状态变更。它能改到的
        // 只有 `PeerInfo.rtt_ms` 与已配对 peer 的 presence 档位——两者都只出现在
        // `Device`，`NetworkStatus` 的任何字段都动不了（`connected_count` /
        // `discovered_count` / `has_connected_bootstrap_peer` 全看 `is_connected` 与
        // `agent_version`，二者只由 PeerConnected/Disconnected/Identified 写）。
        //
        // 所以这里只推设备列表。少了这条分流，几个已连 peer 就足以把全量
        // `NetworkStatus` 推成每几秒一次——而它现在还挂着 `infra_links` 数组，
        // 移动端更要过 uniffi 跨 JNI/Swift 边界。
        NetEvent::PingSuccess { .. } => {
            publish_devices(shared, event_bus).await;
        }
        // identify 会写 `agent_version`，进而改变 `NetworkStatus` 的三个计数位，
        // 必须走全量；其余几个是低频边沿事件，全量的成本可忽略。
        NetEvent::PeerIdentified { .. }
        | NetEvent::Discovered { .. }
        | NetEvent::PathChanged { .. }
        // 升级失败要立刻推一轮：它改变的是设备卡片上那句提示，用户正盯着看
        | NetEvent::LanUpgradeFailed { .. } => {
            publish_devices_and_status(shared, event_bus).await;
        }
        NetEvent::PingFailure { node, error } => {
            // 已配对 peer 的死对端判定在 presence supervisor（顶部 handle_event）完成
            tracing::debug!("Ping 失败 {}: {}", node, error);
        }
    }
    if let Some(device) = refreshed_paired_device {
        // 已配对设备的名称/系统信息来自对端 identify。**落盘与发事件都在 commit 里**
        // （与配对刚达成时走同一个入口），host 不再各自回写——那正是同一个动作在三端
        // 长出三种失败语义的成因。
        //
        // 走 `refresh_paired_device` 而不是 `commit_paired_device`：**刷新不该带「新增」
        // 语义**。上面那句 `publish_devices_and_status(..).await` 是个真实窗口，期间用户
        // 可能刚解除了这台设备的配对；直接 commit 会把它重新写回内存表与库、还发一条
        // `PairedDeviceAdded`，于是刚解除的设备立刻复活。
        //
        // 无返回值：没有用户动作在等这次刷新，写盘失败只意味着「新名字下次启动会退回
        // 旧的」，commit 内部已记 warn。
        shared.pairing.refresh_paired_device(device).await;
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
            )
        })
        .unwrap_or(false);

    // changed 只是"地址/来源有更新"的即时接线路径（避免每次 identify 重复发命令）。
    // 候选进表后，连接与 reservation 的持续维持由 InfraSupervisor 收敛兜底——
    // helper 重启/挂起恢复（changed=false）的重建不再依赖这里。
    if changed {
        let endpoint = shared.endpoint.clone();
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
                // 只记日志、不回写候选表：这是一次**即时接线**失败，意图仍然成立，
                // InfraSupervisor 下一轮 tick 会继续收敛。失败态由 `watch_relays`
                // 表达（它才是内核的实际观测），候选表不背这个状态。
                warn!("注册 LAN Helper 候选失败 {}: {}", node, err);
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
    publish_devices(shared, event_bus).await;
    publish_network_status(shared, event_bus).await;
}

/// 只推设备列表——给「改得到 `Device` 但改不到 `NetworkStatus`」的事件用（见 `PingSuccess`）。
async fn publish_devices<TTransfer>(shared: &SharedNetRefs<TTransfer>, event_bus: &dyn EventBus) {
    let devices = shared.devices.get_devices(DeviceFilter::All);
    let _ = event_bus
        .publish(CoreEvent::DevicesChanged { devices })
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

    // 内核状态（本机地址 / NAT / relay 三态）无对应边沿事件，经 watch 变更驱动状态刷新。
    //
    // `relays_watcher` 不是可选项：`RelayState::Connecting` 与
    // `Failed { last_error }` **没有任何 NetEvent 对应物**——边沿轨只有
    // `RelayReservationAccepted` / `RelayReservationLost` 两个。少了这条订阅，
    // 原生端就永远观测不到「正在连接」，也永远拿不到失败原因，UI 再怎么写都是
    // 一个聚合布尔。Web 端之所以是三端唯一做到逐条 relay 状态的，正因为它直接
    // 订 `relays_changed()` 绕过了 core 这一层。
    //
    // 不会造成推送风暴：`set_relay_state` 用 `send_if_modified` 做了值相等去重，
    // supervisor 走 2s→75s 退避而非每 tick 重发。
    let mut addrs_watcher = shared.endpoint.watch_addrs();
    let mut nat_watcher = shared.endpoint.watch_nat();
    let mut relays_watcher = shared.endpoint.watch_relays();

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
            Some(_) = relays_watcher.updated() => {
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
