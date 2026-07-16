//! presence 状态机与后台维持任务。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use swarm_p2p_core::NodeEvent;
use swarm_p2p_core::libp2p::kad::Record;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use tokio::time::Instant;

use super::{ONLINE_RECORD_TTL_SECS, OnlineRecord, RelayHint};
use crate::device::{OsInfo, PairedDeviceInfo};
use crate::dht_key;
use crate::network::candidates::{BootstrapCandidateManager, CandidateHealth};
use crate::protocol::{AppNetClient, AppRequest};

/// per-paired-peer 的 presence 状态。
///
/// 设备列表的在线判定：`Connected | Probing → Online`，`Unreachable → Offline`。
#[derive(Debug, Clone, Copy)]
pub enum PresenceState {
    /// 存在活跃 libp2p 连接
    Connected,
    /// 连接刚断开，宽限期内退避重拨中（UI 仍呈现在线）
    Probing {
        since: Instant,
        next_dial_at: Instant,
        attempts: u32,
    },
    /// 宽限期内未拨通，低频重探中（UI 呈现离线）
    Unreachable { next_probe_at: Instant },
}

impl PresenceState {
    /// 对外（设备列表）是否呈现为在线
    pub fn is_online(&self) -> bool {
        !matches!(self, PresenceState::Unreachable { .. })
    }
}

/// 共享 presence 状态表：Supervisor 写，DeviceManager 读。
pub type PresenceMap = Arc<DashMap<PeerId, PresenceState>>;

/// presence 时间参数（测试可注入缩短版）
#[derive(Debug, Clone, Copy)]
pub struct PresenceTimings {
    /// 状态机推进周期
    pub tick: Duration,
    /// 断连宽限期：期间退避重拨、UI 维持在线
    pub grace: Duration,
    /// Unreachable 设备的基础重探周期（附加 per-peer 抖动）
    pub probe_interval: Duration,
    /// 重探周期的 per-peer 抖动上限（秒）
    pub probe_jitter_secs: u64,
    /// OnlineRecord 刷新周期（≈ TTL/2）
    pub announce_interval: Duration,
}

impl Default for PresenceTimings {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(1),
            grace: Duration::from_secs(15),
            probe_interval: Duration::from_secs(75),
            probe_jitter_secs: 15,
            announce_interval: Duration::from_secs(ONLINE_RECORD_TTL_SECS / 2),
        }
    }
}

/// 宽限期内的重拨退避：立即（断连时）→ 2s → 5s → 10s
fn dial_backoff(attempts: u32) -> Duration {
    match attempts {
        0 | 1 => Duration::from_secs(2),
        2 => Duration::from_secs(5),
        _ => Duration::from_secs(10),
    }
}

/// 判定死对端所需的连续 PingFailure 事件数。
///
/// 只对 TCP 连接真正生效：QUIC 传输层自带 10s idle 判死（先于本机制），
/// TCP/yamux 无任何传输层判死，死对端全靠 ping 失败暴露。
///
/// 注意 libp2p-ping handler 会静默吞掉第 1 次失败（兼容每次新开 substream
/// 的对端），从第 2 次连续失败起才上报事件——所以阈值 2 个事件 ≈ 协议层
/// 连续 3 次失败 ≈ 40s（15s 间隔 + 10s 超时），加 15s 宽限后约 1 分钟判离线。
/// 阈值取 2 而非 1 是为了避免大传输压满链路时 ping 偶发超时误杀活连接。
const PING_FAILURE_THRESHOLD: u32 = 2;

/// 在线记录携带的中继提示上限（防 record 膨胀）
const MAX_RELAY_HINTS: usize = 3;

/// presence 唯一大脑。
///
/// 事件输入走 [`handle_event`](Self::handle_event)（core 事件循环同步调用），
/// 定时推进走 [`tick`](Self::tick)（由 [`run`](Self::run) 的后台任务驱动）。
pub struct PresenceSupervisor {
    client: AppNetClient,
    peer_id: PeerId,
    paired: Arc<DashMap<PeerId, PairedDeviceInfo>>,
    presence: PresenceMap,
    /// 候选表（构建在线记录的 relay hint 用）
    candidates: Arc<RwLock<BootstrapCandidateManager>>,
    /// 连续 ping 失败计数（PingSuccess / 断连时清零）
    ping_failures: DashMap<PeerId, u32>,
    /// 在线记录需要重发（地址集/reservation 变化或上次发布失败）
    announce_dirty: AtomicBool,
    /// announce 连续失败计数（重试退避用）
    announce_fail_streak: AtomicU32,
    timings: PresenceTimings,
}

impl PresenceSupervisor {
    pub fn new(
        client: AppNetClient,
        peer_id: PeerId,
        paired: Arc<DashMap<PeerId, PairedDeviceInfo>>,
        presence: PresenceMap,
        candidates: Arc<RwLock<BootstrapCandidateManager>>,
    ) -> Self {
        Self {
            client,
            peer_id,
            paired,
            presence,
            candidates,
            ping_failures: DashMap::new(),
            announce_dirty: AtomicBool::new(false),
            announce_fail_streak: AtomicU32::new(0),
            timings: PresenceTimings::default(),
        }
    }

    /// 幂等地把 peer 加入连接保活白名单（behaviour 侧去重）
    fn spawn_keep_alive(&self, peer: PeerId) {
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = client.set_keep_alive(peer, true).await;
        });
    }

    // === 事件折叠（core 事件循环调用） ===

    /// 消费连接/ping 事件，折叠 presence 状态。仅关心已配对 peer。
    pub fn handle_event(&self, event: &NodeEvent<AppRequest>) {
        match event {
            NodeEvent::PeerConnected { peer_id } if self.paired.contains_key(peer_id) => {
                if self
                    .presence
                    .insert(*peer_id, PresenceState::Connected)
                    .is_none()
                {
                    // 首次建 entry（新配对后事件先于 reconcile 到达）：
                    // 立即补保活白名单，避免 reconcile 因 entry 已存在而跳过
                    self.spawn_keep_alive(*peer_id);
                }
            }
            // 防御性收敛：有 ping 必有活跃连接（补 PeerConnected 事件错过的场景）
            NodeEvent::PingSuccess { peer_id, .. } if self.paired.contains_key(peer_id) => {
                self.ping_failures.remove(peer_id);
                if !matches!(
                    self.presence.get(peer_id).map(|e| *e.value()),
                    Some(PresenceState::Connected)
                ) && self
                    .presence
                    .insert(*peer_id, PresenceState::Connected)
                    .is_none()
                {
                    self.spawn_keep_alive(*peer_id);
                }
            }
            // 死对端检测（TCP 兜底）：连续失败达阈值 → 主动断连 → 走 Probing 流程。
            // QUIC 死对端由传输层 10s idle 先行判死，不会走到阈值。
            NodeEvent::PingFailure { peer_id, error } if self.paired.contains_key(peer_id) => {
                let failures = {
                    let mut entry = self.ping_failures.entry(*peer_id).or_insert(0);
                    *entry += 1;
                    *entry
                };
                if failures >= PING_FAILURE_THRESHOLD
                    && matches!(
                        self.presence.get(peer_id).map(|e| *e.value()),
                        Some(PresenceState::Connected)
                    )
                {
                    tracing::info!(
                        "已配对设备 {peer_id} 连续 {failures} 次 ping 失败（{error}），主动断连重探"
                    );
                    self.ping_failures.remove(peer_id);
                    let client = self.client.clone();
                    let peer = *peer_id;
                    tokio::spawn(async move {
                        let _ = client.disconnect(peer).await;
                    });
                }
            }
            NodeEvent::PeerDisconnected { peer_id } if self.paired.contains_key(peer_id) => {
                self.ping_failures.remove(peer_id);
                let now = Instant::now();
                self.presence.insert(
                    *peer_id,
                    PresenceState::Probing {
                        since: now,
                        next_dial_at: now + dial_backoff(1),
                        attempts: 1,
                    },
                );
                // 断连即首拨（第 0 秒尝试）；拨通由 PeerConnected 收敛回 Connected
                let client = self.client.clone();
                let peer = *peer_id;
                tokio::spawn(async move {
                    let _ = client.dial(peer).await;
                });
            }
            // 可达性事实变化 → 在线记录标脏，run 循环按退避立即重发
            NodeEvent::Listening { .. }
            | NodeEvent::RelayReservationAccepted { .. }
            | NodeEvent::RelayReservationLost { .. }
            | NodeEvent::NatStatusChanged { .. } => {
                self.announce_dirty.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    // === 状态机推进（定时任务调用） ===

    /// 推进一轮状态机：白名单对账、宽限重拨、超时判离线、低频重探。
    ///
    /// 返回是否有设备在本轮转为 Unreachable（调用方据此推送 DevicesChanged）。
    /// `is_connected` 用于新配对设备的初始状态判定（解耦 DeviceManager，便于测试）。
    pub async fn tick(
        &self,
        now: Instant,
        is_connected: &(dyn Fn(&PeerId) -> bool + Sync),
    ) -> bool {
        self.reconcile_whitelist(now, is_connected);

        let mut went_offline = false;
        let peers: Vec<PeerId> = self.presence.iter().map(|e| *e.key()).collect();
        for peer in peers {
            // 逐个 get_mut 并在锁内复核状态，避免覆盖事件线程刚写入的 Connected
            let Some(mut entry) = self.presence.get_mut(&peer) else {
                continue;
            };
            match *entry.value() {
                PresenceState::Connected => {}
                PresenceState::Probing {
                    since,
                    next_dial_at,
                    attempts,
                } => {
                    if now.duration_since(since) >= self.timings.grace {
                        *entry.value_mut() = PresenceState::Unreachable {
                            next_probe_at: now + self.probe_interval(&peer),
                        };
                        went_offline = true;
                    } else if now >= next_dial_at {
                        *entry.value_mut() = PresenceState::Probing {
                            since,
                            next_dial_at: now + dial_backoff(attempts + 1),
                            attempts: attempts + 1,
                        };
                        drop(entry);
                        let client = self.client.clone();
                        tokio::spawn(async move {
                            let _ = client.dial(peer).await;
                        });
                    }
                }
                PresenceState::Unreachable { next_probe_at } => {
                    if now >= next_probe_at {
                        *entry.value_mut() = PresenceState::Unreachable {
                            next_probe_at: now + self.probe_interval(&peer),
                        };
                        drop(entry);
                        self.spawn_probe(peer);
                    }
                }
            }
        }
        went_offline
    }

    /// 白名单对账：paired 集合与 presence 集合互相收敛。
    ///
    /// - 新配对 → 进保活白名单 + 建立初始状态（已连接则 Connected，否则立即重探）
    /// - 解除配对 → 出白名单 + 断开连接 + 移除状态
    fn reconcile_whitelist(&self, now: Instant, is_connected: &(dyn Fn(&PeerId) -> bool + Sync)) {
        for entry in self.paired.iter() {
            let peer = *entry.key();
            if !self.presence.contains_key(&peer) {
                let initial = if is_connected(&peer) {
                    PresenceState::Connected
                } else {
                    PresenceState::Unreachable { next_probe_at: now }
                };
                self.presence.entry(peer).or_insert(initial);
                self.spawn_keep_alive(peer);
            }
        }

        let removed: Vec<PeerId> = self
            .presence
            .iter()
            .map(|e| *e.key())
            .filter(|p| !self.paired.contains_key(p))
            .collect();
        for peer in removed {
            self.presence.remove(&peer);
            self.ping_failures.remove(&peer);
            let client = self.client.clone();
            tokio::spawn(async move {
                let _ = client.set_keep_alive(peer, false).await;
                let _ = client.disconnect(peer).await;
            });
        }
    }

    /// 基础重探周期 + per-peer 确定性抖动（避免多设备同拍重探）
    fn probe_interval(&self, peer: &PeerId) -> Duration {
        let hash = peer
            .to_bytes()
            .iter()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
        self.timings.probe_interval
            + Duration::from_secs(hash % self.timings.probe_jitter_secs.max(1))
    }

    /// 重探一台离线设备（多步编排）：
    ///
    /// 1. 查 DHT 在线记录，注册 direct+circuit 地址后直拨；
    /// 2. 直拨失败且记录携带 relay hint → 逐个先与 relay 建连
    ///    （触发 DCUtR 自升级），再拨目标的 circuit 地址；
    /// 3. 失败原因分级记录（无记录/地址不可拨/relay 不可达），不静默。
    ///
    /// DHT 无记录时仍尝试直接 dial：地址簿可能还留有 mDNS 注册的局域网地址
    /// （无公网 bootstrap 的纯局域网场景 DHT 记录不可用）。
    fn spawn_probe(&self, peer: PeerId) {
        let client = self.client.clone();
        tokio::spawn(async move {
            let record = match client
                .get_record(dht_key::online_key(&peer.to_bytes()))
                .await
            {
                Ok(result) => {
                    let record = result.record;
                    let expired = record
                        .expires
                        .map(|e| e < std::time::Instant::now())
                        .unwrap_or(false);
                    if expired {
                        None
                    } else {
                        serde_json::from_slice::<OnlineRecord>(&record.value).ok()
                    }
                }
                Err(_) => None,
            };

            let Some(online) = record else {
                tracing::debug!("重探 {peer}: 无在线记录，地址簿兜底直拨");
                let _ = client.dial(peer).await;
                return;
            };

            let addrs = online.dialable_addrs();
            if !addrs.is_empty() {
                let _ = client.add_peer_addrs(peer, addrs).await;
            }
            if client.dial(peer).await.is_ok() {
                return;
            }

            // 直拨失败 → relay hint 多步恢复：先修与 relay 的直连再拨 circuit
            for hint in online.relays.iter().take(MAX_RELAY_HINTS) {
                if hint.addrs.is_empty() {
                    continue;
                }
                let _ = client
                    .add_peer_addrs(hint.peer_id, hint.addrs.clone())
                    .await;
                if let Err(e) = client.dial(hint.peer_id).await {
                    tracing::debug!("重探 {peer}: relay {} 不可达: {e}", hint.peer_id);
                    continue;
                }
                match client.dial(peer).await {
                    Ok(()) => return,
                    Err(e) => {
                        tracing::debug!("重探 {peer}: 经 relay {} 拨号失败: {e}", hint.peer_id)
                    }
                }
            }
            tracing::debug!(
                "重探 {peer}: 全部地址不可拨（{} 个 hint 已尝试）",
                online.relays.len()
            );
        });
    }

    // === 在线宣告（DHT OnlineRecord） ===

    /// 宣布上线：发布结构化的可达性声明到 DHT（TTL 见 [`ONLINE_RECORD_TTL_SECS`]）
    pub async fn announce_online(&self) -> crate::AppResult<()> {
        let addrs = self.client.get_addrs().await?;
        let (direct_addrs, relay_addrs) = classify_announce_addrs(addrs);
        let record_data = build_online_record(direct_addrs, relay_addrs, self.relay_hints());
        self.client
            .put_record(Record {
                key: dht_key::online_key(&self.peer_id.to_bytes()),
                value: serde_json::to_vec(&record_data)?,
                publisher: Some(self.peer_id),
                expires: Some(
                    std::time::Instant::now() + Duration::from_secs(ONLINE_RECORD_TTL_SECS),
                ),
            })
            .await?;
        Ok(())
    }

    /// 从候选表取活跃 relay 作为中继提示（≤3，供对端先修 relay 直连再拨 circuit）
    fn relay_hints(&self) -> Vec<RelayHint> {
        let Ok(candidates) = self.candidates.read() else {
            return Vec::new();
        };
        candidates
            .snapshot()
            .into_iter()
            .filter(|c| c.roles.relay_server && matches!(c.health, CandidateHealth::RelayReady))
            .take(MAX_RELAY_HINTS)
            .map(|c| RelayHint {
                peer_id: c.peer_id,
                addrs: c
                    .addrs
                    .into_iter()
                    .filter(|a| !swarm_p2p_core::addr::is_loopback_or_unspecified(a))
                    .collect(),
            })
            .filter(|hint| !hint.addrs.is_empty())
            .collect()
    }

    /// 宣布下线：从 DHT 移除在线记录（节点停止时调用）
    pub async fn announce_offline(&self) -> crate::AppResult<()> {
        self.client
            .remove_record(dht_key::online_key(&self.peer_id.to_bytes()))
            .await?;
        Ok(())
    }

    fn spawn_announce(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            match this.announce_online().await {
                Ok(()) => {
                    this.announce_fail_streak.store(0, Ordering::Relaxed);
                }
                Err(e) => {
                    // 失败保持脏标记，run 循环按退避重试（而非死等 150s 周期）
                    this.announce_fail_streak.fetch_add(1, Ordering::Relaxed);
                    this.announce_dirty.store(true, Ordering::Relaxed);
                    tracing::debug!("announce_online 失败（将退避重试）: {e}");
                }
            }
        });
    }

    // === 后台维持任务 ===

    /// presence 后台任务：启动序列（保活装载 → 宣告 → bootstrap → 首轮重探）+ 周期推进。
    ///
    /// 由 core 事件循环启动时拉起，随 `cancel_token` 结束。
    pub async fn run<TTransfer>(
        self: Arc<Self>,
        shared: crate::network::SharedNetRefs<TTransfer>,
        event_bus: Arc<dyn crate::host::EventBus>,
    ) where
        TTransfer: Send + Sync + 'static,
    {
        let now = Instant::now();
        // 启动装载：全部已配对设备进保活白名单 + 立即首轮重探（吸收原 check_paired_online）
        for entry in self.paired.iter() {
            let peer = *entry.key();
            self.presence
                .entry(peer)
                .or_insert(PresenceState::Unreachable { next_probe_at: now });
            self.spawn_keep_alive(peer);
        }

        // 宣告 → bootstrap（沿用原 host 启动序列的顺序；bootstrap 加超时防挂）
        self.spawn_announce();
        let _ = tokio::time::timeout(Duration::from_secs(30), self.client.bootstrap()).await;

        let mut last_announce_attempt = Instant::now();
        let mut interval = tokio::time::interval(self.timings.tick);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = shared.cancel_token.cancelled() => break,
                _ = interval.tick() => {
                    let now = Instant::now();
                    let went_offline = self
                        .tick(now, &|p| shared.devices.is_connected(p))
                        .await;
                    if went_offline {
                        crate::network::event_loop::publish_devices_and_status(
                            &shared,
                            event_bus.as_ref(),
                        )
                        .await;
                    }
                    // 事件驱动（脏标记 + 失败退避）优先，150s 周期兜底
                    let since = now.duration_since(last_announce_attempt);
                    let dirty = self.announce_dirty.load(Ordering::Relaxed);
                    let retry_after =
                        announce_backoff(self.announce_fail_streak.load(Ordering::Relaxed));
                    if (dirty && since >= retry_after) || since >= self.timings.announce_interval {
                        self.announce_dirty.store(false, Ordering::Relaxed);
                        last_announce_attempt = now;
                        self.spawn_announce();
                    }
                }
            }
        }
        tracing::info!("presence supervisor 退出");
    }
}

/// 脏标记触发的重发去抖/失败退避：2s 起步，随连续失败翻倍，上限 30s
fn announce_backoff(fail_streak: u32) -> Duration {
    let secs = 2u64.saturating_mul(1 << fail_streak.min(4));
    Duration::from_secs(secs.min(30))
}

/// 构造在线宣告记录。
///
/// **这条记录发布到公共 DHT**：key = `SHA256(NS‖peer_id)` 可由公开信息算出，记录本身
/// 无签名，任何加入网络的节点都能查。所以它只能携带「让已配对设备拨得通」所必需的
/// 地址，不能携带任何设备身份信息。
///
/// 抽成纯函数正是为了让这条约束可测——见 `online_record_must_not_carry_device_info`。
fn build_online_record(
    direct_addrs: Vec<Multiaddr>,
    relay_addrs: Vec<Multiaddr>,
    relays: Vec<RelayHint>,
) -> OnlineRecord {
    OnlineRecord {
        // 死字段（读取端只用 dialable_addrs，从不消费它），发空占位仅为 wire 兼容。
        // 详见 OsInfo::redacted 的文档。
        os_info: OsInfo::redacted(),
        direct_addrs,
        relay_addrs,
        relays,
        timestamp: chrono::Utc::now().timestamp(),
    }
}

/// 把本机地址集分类为可发布的（direct, relay circuit）两组。
///
/// - loopback/unspecified 剔除（对任何对端无意义）
/// - 恰好一跳 circuit → relay 组；多跳 circuit 剔除（libp2p 硬拒）
/// - 私网地址保留在 direct 组（跨子网 LAN 可用，跨网拨快速失败无害）
fn classify_announce_addrs(addrs: Vec<Multiaddr>) -> (Vec<Multiaddr>, Vec<Multiaddr>) {
    use swarm_p2p_core::addr::{circuit_hops, is_loopback_or_unspecified};
    let mut direct = Vec::new();
    let mut relay = Vec::new();
    for addr in addrs {
        if is_loopback_or_unspecified(&addr) {
            continue;
        }
        match circuit_hops(&addr) {
            0 => direct.push(addr),
            1 => relay.push(addr),
            _ => {} // 多跳 circuit：任何 libp2p 节点都拨不了
        }
    }
    (direct, relay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AppRequest, AppResponse};
    use swarm_p2p_core::libp2p::identity::Keypair;

    /// 在线宣告记录发布到**公共 DHT**：key = `SHA256(NS‖peer_id)` 由公开信息可算，
    /// 记录无签名，任何加入网络的节点都能查。它绝不能携带设备身份信息——尤其是
    /// `hostname`（`OsInfo::default()` 读 `COMPUTERNAME`/`HOSTNAME`，常含真名）。
    ///
    /// 若有人把 `build_online_record` 里的 `OsInfo::redacted()` 改回 `OsInfo::default()`，
    /// 本测试会失败。**那不是测试过时，是把每 150 秒广播一次主机名的行为改了回来。**
    #[test]
    fn online_record_must_not_carry_device_info() {
        let rec = build_online_record(vec![], vec![], vec![]);

        assert_eq!(
            rec.os_info,
            OsInfo::redacted(),
            "在线宣告发布到任何人可查的公共 DHT，不得携带设备信息"
        );

        // wire 兼容的另一半：字段本身必须仍在序列化结果里。
        // OsInfo 的 hostname/os/platform/arch 都没有 #[serde(default)]，
        // 直接删字段会让存量客户端整条记录反序列化失败 → 丢掉 direct_addrs → 退化成盲拨。
        let json = serde_json::to_string(&rec).unwrap();
        for key in ["hostname", "os", "platform", "arch"] {
            assert!(
                json.contains(key),
                "wire 格式必须保留 `{key}` 键，否则存量客户端无法反序列化整条记录"
            );
        }
    }

    struct TestCtx {
        supervisor: PresenceSupervisor,
        paired: Arc<DashMap<PeerId, PairedDeviceInfo>>,
        presence: PresenceMap,
        // 事件接收端必须存活，否则节点事件循环提前退出
        _events: swarm_p2p_core::EventReceiver<AppRequest>,
    }

    fn test_ctx() -> TestCtx {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from_public_key(&keypair.public());
        let config = swarm_p2p_core::NodeConfig::new("/swarmdrop-test/1.0.0", "test/1.0.0")
            .with_listen_addrs(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
            .with_mdns(false)
            .with_relay_client(false)
            .with_dcutr(false)
            .with_autonat(false);
        let (client, events, _dc) =
            swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, config)
                .expect("start test node");

        let paired: Arc<DashMap<PeerId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        let presence: PresenceMap = Arc::new(DashMap::new());
        let candidates = Arc::new(RwLock::new(BootstrapCandidateManager::new(
            crate::network::DiscoveryMode::Auto,
            true,
        )));
        let supervisor = PresenceSupervisor::new(
            client,
            peer_id,
            paired.clone(),
            presence.clone(),
            candidates,
        );
        TestCtx {
            supervisor,
            paired,
            presence,
            _events: events,
        }
    }

    fn pair(ctx: &TestCtx) -> PeerId {
        let peer = PeerId::from_public_key(&Keypair::generate_ed25519().public());
        ctx.paired
            .insert(peer, PairedDeviceInfo::new(peer, OsInfo::default(), 0));
        peer
    }

    fn state_of(ctx: &TestCtx, peer: &PeerId) -> Option<PresenceState> {
        ctx.presence.get(peer).map(|e| *e.value())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transient_disconnect_stays_online_and_reconnects() {
        let ctx = test_ctx();
        let peer = pair(&ctx);
        ctx.presence.insert(peer, PresenceState::Connected);

        // 断连 → Probing（宽限期内仍在线，不推离线）
        ctx.supervisor
            .handle_event(&NodeEvent::PeerDisconnected { peer_id: peer });
        let state = state_of(&ctx, &peer).unwrap();
        assert!(matches!(state, PresenceState::Probing { .. }));
        assert!(state.is_online(), "宽限期内必须仍呈现在线");

        let went_offline = ctx.supervisor.tick(Instant::now(), &|_| false).await;
        assert!(!went_offline, "宽限期内 tick 不得判离线");

        // 拨通 → 回 Connected
        ctx.supervisor
            .handle_event(&NodeEvent::PeerConnected { peer_id: peer });
        assert!(matches!(
            state_of(&ctx, &peer),
            Some(PresenceState::Connected)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn grace_timeout_marks_unreachable() {
        let ctx = test_ctx();
        let peer = pair(&ctx);
        ctx.supervisor
            .handle_event(&NodeEvent::PeerDisconnected { peer_id: peer });

        // 快进到宽限期之后
        let later = Instant::now() + ctx.supervisor.timings.grace + Duration::from_secs(1);
        let went_offline = ctx.supervisor.tick(later, &|_| false).await;
        assert!(went_offline, "宽限超时必须上报离线");
        let state = state_of(&ctx, &peer).unwrap();
        assert!(matches!(state, PresenceState::Unreachable { .. }));
        assert!(!state.is_online());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unpair_removes_presence_state() {
        let ctx = test_ctx();
        let peer = pair(&ctx);
        ctx.presence.insert(peer, PresenceState::Connected);

        ctx.paired.remove(&peer);
        ctx.supervisor.tick(Instant::now(), &|_| false).await;
        assert!(
            state_of(&ctx, &peer).is_none(),
            "解除配对后 presence 状态必须清理"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unreachable_probe_reschedules() {
        let ctx = test_ctx();
        let peer = pair(&ctx);
        let now = Instant::now();
        ctx.presence
            .insert(peer, PresenceState::Unreachable { next_probe_at: now });

        let went_offline = ctx.supervisor.tick(now, &|_| false).await;
        assert!(!went_offline, "已离线设备的重探不重复上报离线");
        match state_of(&ctx, &peer).unwrap() {
            PresenceState::Unreachable { next_probe_at } => {
                assert!(next_probe_at > now, "重探后必须重新排期");
            }
            other => panic!("状态不应改变: {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consecutive_ping_failures_trigger_dead_peer_disconnect() {
        let ctx = test_ctx();
        let peer = pair(&ctx);
        ctx.presence.insert(peer, PresenceState::Connected);

        let fail = NodeEvent::PingFailure {
            peer_id: peer,
            error: "Ping timeout".into(),
        };

        // 第 1 次失败：计数累积，不动作
        ctx.supervisor.handle_event(&fail);
        assert_eq!(
            ctx.supervisor.ping_failures.get(&peer).map(|e| *e.value()),
            Some(1)
        );

        // ping 恢复：计数清零
        ctx.supervisor.handle_event(&NodeEvent::PingSuccess {
            peer_id: peer,
            rtt_ms: 5,
        });
        assert!(ctx.supervisor.ping_failures.get(&peer).is_none());

        // 连续 2 次失败：触发主动断连（计数清零，等待 PeerDisconnected 进 Probing）
        ctx.supervisor.handle_event(&fail);
        ctx.supervisor.handle_event(&fail);
        assert!(
            ctx.supervisor.ping_failures.get(&peer).is_none(),
            "达到阈值后计数应清零"
        );
        assert!(
            matches!(state_of(&ctx, &peer), Some(PresenceState::Connected)),
            "状态转换应等待真实的 PeerDisconnected 事件"
        );
    }

    #[test]
    fn classify_filters_undialable_and_multihop_addrs() {
        let addrs: Vec<Multiaddr> = [
            "/ip4/192.168.1.20/tcp/4001",                     // 私网直连：保留
            "/ip4/203.0.113.7/udp/4001/quic-v1",              // 公网直连：保留
            "/ip4/127.0.0.1/tcp/4001",                        // loopback：剔除
            "/ip4/0.0.0.0/tcp/4001",                          // unspecified：剔除
            // 合法一跳 circuit → relay 组
            "/ip4/192.168.1.5/tcp/4001/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp/p2p-circuit",
            // 非法二跳 circuit：剔除
            "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp/p2p-circuit/p2p/12D3KooWQYhTNQdmr3ArTeUHRYzFg94BKyTkoWBDWez9kSCVe2Xo/p2p-circuit",
        ]
        .into_iter()
        .map(|a| a.parse().unwrap())
        .collect();

        let (direct, relay) = classify_announce_addrs(addrs);

        assert_eq!(direct.len(), 2, "私网+公网直连保留: {direct:?}");
        assert_eq!(relay.len(), 1, "仅一跳 circuit 进 relay 组: {relay:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reachability_events_mark_announce_dirty() {
        let ctx = test_ctx();
        assert!(!ctx.supervisor.announce_dirty.load(Ordering::Relaxed));

        ctx.supervisor
            .handle_event(&NodeEvent::RelayReservationAccepted {
                relay_peer_id: PeerId::from_public_key(&Keypair::generate_ed25519().public()),
                renewal: false,
            });
        assert!(
            ctx.supervisor.announce_dirty.load(Ordering::Relaxed),
            "reservation 建立必须标脏在线记录"
        );

        ctx.supervisor
            .announce_dirty
            .store(false, Ordering::Relaxed);
        ctx.supervisor.handle_event(&NodeEvent::Listening {
            addr: "/ip4/192.168.1.2/tcp/4001".parse().unwrap(),
        });
        assert!(
            ctx.supervisor.announce_dirty.load(Ordering::Relaxed),
            "监听地址变化必须标脏在线记录"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconcile_initializes_new_paired_peers() {
        let ctx = test_ctx();
        let connected_peer = pair(&ctx);
        let offline_peer = pair(&ctx);

        ctx.supervisor
            .tick(Instant::now(), &|p| *p == connected_peer)
            .await;

        assert!(matches!(
            state_of(&ctx, &connected_peer),
            Some(PresenceState::Connected)
        ));
        assert!(matches!(
            state_of(&ctx, &offline_peer),
            Some(PresenceState::Unreachable { .. })
        ));
    }
}
