//! 基础设施链路的收敛状态机。

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use n0_future::time::Instant;
use swarmdrop_net::{Addr, Endpoint, InfraRoles, NetEvent, NodeAddr, NodeId};

use crate::device::OsInfo;
use crate::network::candidates::{
    BootstrapCandidate, BootstrapCandidateManager, BootstrapCandidateSource, CandidateRoles,
    CandidateScope,
};

/// 学习型候选上限（防陌生节点撑爆候选表）
const MAX_LEARNED_CANDIDATES: usize = 4;

/// reservation 重建退避：2s → 5s → 10s → 30s，上限 75s
fn rebuild_backoff(attempts: u32) -> Duration {
    match attempts {
        0 | 1 => Duration::from_secs(2),
        2 => Duration::from_secs(5),
        3 => Duration::from_secs(10),
        4 => Duration::from_secs(30),
        _ => Duration::from_secs(75),
    }
}

/// 某个 relay 候选的收敛状态
#[derive(Debug, Clone, Copy)]
struct RelayLinkState {
    /// 当前是否持有活跃 reservation
    reservation_active: bool,
    next_attempt_at: Instant,
    attempts: u32,
    /// 上次看到的候选 last_seen：候选被重新发现（地址刷新）时重置退避，
    /// 避免"挂起期间退避涨满、恢复后干等一分钟"
    candidate_seen: chrono::DateTime<chrono::Utc>,
}

/// 基础设施收敛大脑。
///
/// 事件输入走 [`handle_event`](Self::handle_event)（core 事件循环同步调用），
/// 定时推进走 [`tick`](Self::tick)（由 [`run`](Self::run) 的后台任务驱动）。
pub struct InfraSupervisor {
    endpoint: Endpoint,
    candidates: Arc<RwLock<BootstrapCandidateManager>>,
    /// 公网可达性设置：false 时不对 Public 范围候选做 reservation
    public_reachability: bool,
    /// relay 候选的收敛状态（key = 候选 peer）
    links: DashMap<NodeId, RelayLinkState>,
}

impl InfraSupervisor {
    pub fn new(
        endpoint: Endpoint,
        candidates: Arc<RwLock<BootstrapCandidateManager>>,
        public_reachability: bool,
    ) -> Self {
        Self {
            endpoint,
            candidates,
            public_reachability,
            links: DashMap::new(),
        }
    }

    /// 该候选是否应维持 reservation
    fn wants_reservation(&self, candidate: &BootstrapCandidate) -> bool {
        candidate.roles.relay_server
            && (matches!(candidate.scope, CandidateScope::Lan) || self.public_reachability)
    }

    /// 注销一个基础设施节点的收敛状态（候选条目由调用方一并清除）。
    ///
    /// 清掉 `links` 后，后续 tick 对它不再有正向收敛动作；与在途注册任务的
    /// 竞态由 [`tick`](Self::tick) 的反向收敛闭合（差集发现即注销）。
    pub fn remove(&self, peer_id: NodeId) {
        self.links.remove(&peer_id);
    }

    // === 事件折叠（core 事件循环调用） ===

    pub fn handle_event(&self, event: &NetEvent) {
        match event {
            NetEvent::RelayReservationAccepted { relay, .. } => {
                self.links.insert(
                    *relay,
                    RelayLinkState {
                        reservation_active: true,
                        next_attempt_at: Instant::now(),
                        attempts: 0,
                        candidate_seen: chrono::Utc::now(),
                    },
                );
            }
            NetEvent::RelayReservationLost { relay } => {
                // 轮数只在策略层内账（RelayState 不再携带），诊断走日志
                let attempts = self.links.get(relay).map(|l| l.attempts).unwrap_or(0);
                tracing::info!("relay reservation 丢失: {relay}（已尝试 {attempts} 轮），进入重建");
                // 只翻可用位，保留既有退避进度——reservation 被 relay 拒绝时
                // 每次尝试都会产生一次 Lost，无条件清零会退化成 1-2s 重试风暴。
                // attempts 仅由 Accepted（健康恢复）与候选 last_seen 刷新（重新发现）归零。
                self.links
                    .entry(*relay)
                    .and_modify(|link| link.reservation_active = false)
                    .or_insert(RelayLinkState {
                        reservation_active: false,
                        next_attempt_at: Instant::now(),
                        attempts: 0,
                        candidate_seen: chrono::Utc::now(),
                    });
            }
            // 学习型候选：识别基础设施 agent 自动纳管
            NetEvent::PeerIdentified {
                node, agent, addrs, ..
            } if OsInfo::is_bootstrap_agent(agent) => {
                self.learn_candidate(*node, addrs);
            }
            _ => {}
        }
    }

    /// 把运行时认识的基础设施节点纳入候选表（Learned 来源）。
    fn learn_candidate(&self, peer_id: NodeId, listen_addrs: &[Addr]) {
        let addrs = usable_public_addrs(listen_addrs);
        if addrs.is_empty() {
            return;
        }
        let Ok(mut candidates) = self.candidates.write() else {
            return;
        };
        // 已知候选只做地址合并；新候选受数量上限约束
        if !candidates.contains(peer_id) && candidates.learned_count() >= MAX_LEARNED_CANDIDATES {
            return;
        }
        let changed = candidates.upsert(
            peer_id,
            addrs.clone(),
            BootstrapCandidateSource::Learned,
            CandidateRoles::kad_and_relay(),
            CandidateScope::Public,
        );
        drop(candidates);

        if changed {
            tracing::info!("学习到基础设施节点 {peer_id}（{} 个公网地址）", addrs.len());
            // 即时 kad 接线；reservation 交给 tick 按 public_reachability 决策
            let endpoint = self.endpoint.clone();
            n0_future::task::spawn(async move {
                let _ = endpoint
                    .add_infrastructure_peer(
                        NodeAddr::with_addrs(peer_id, addrs),
                        InfraRoles {
                            kad_server: true,
                            relay: false,
                        },
                    )
                    .await;
            });
        }
    }

    // === 收敛推进（定时任务调用） ===

    /// 对每个应持有 reservation 的候选做一轮收敛检查（双向）。
    pub fn tick(&self, now: Instant) {
        let snapshot = self
            .candidates
            .read()
            .map(|c| c.snapshot())
            .unwrap_or_default();

        // 反向收敛：内核有 relay 登记（watch_relays 条目）而候选表已无该 peer
        // → 幂等注销，条目消失前每轮重发。终态一致由环保证：注销与在途注册
        // 任务竞态导致登记短暂复活时，watch 重现条目、下一轮差集必然再次清理
        // ——判据前提是候选表只经显式撤销移除（无自动过期清出，见 spec）。
        let desired: HashSet<NodeId> = snapshot.iter().map(|c| c.peer_id).collect();
        let observed: Vec<NodeId> = self
            .endpoint
            .watch_relays()
            .with(|map| map.keys().copied().collect());
        for peer in observed.into_iter().filter(|p| !desired.contains(p)) {
            self.links.remove(&peer);
            let endpoint = self.endpoint.clone();
            n0_future::task::spawn(async move {
                tracing::info!("反向收敛：注销已无候选的 relay 登记 {peer}");
                let _ = endpoint.remove_infrastructure_peer(peer).await;
            });
        }

        for candidate in snapshot {
            if !self.wants_reservation(&candidate) {
                continue;
            }
            let mut link = self
                .links
                .entry(candidate.peer_id)
                .or_insert(RelayLinkState {
                    reservation_active: false,
                    next_attempt_at: now,
                    attempts: 0,
                    candidate_seen: candidate.last_seen,
                });
            // 候选被重新发现（如 helper 重启后 mDNS 刷新地址）→ 重置退避立即收敛
            if candidate.last_seen > link.candidate_seen {
                link.candidate_seen = candidate.last_seen;
                if !link.reservation_active {
                    link.attempts = 0;
                    link.next_attempt_at = now;
                }
            }
            if link.reservation_active || now < link.next_attempt_at {
                continue;
            }
            link.attempts += 1;
            link.next_attempt_at = now + rebuild_backoff(link.attempts);
            let attempts = link.attempts;
            drop(link);

            let endpoint = self.endpoint.clone();
            let peer = candidate.peer_id;
            let addrs = candidate.addrs.clone();
            let roles: InfraRoles = candidate.roles.into();
            n0_future::task::spawn(async move {
                // 与注销的竞态无需在此防护：即便本任务复活了已注销的登记，
                // 反向收敛（tick 开头的差集）会在下一轮清理——终态一致由环保证
                tracing::debug!("收敛基础设施链路: {peer}（第 {attempts} 次尝试）");
                // 全角色注册：kad 重接线 + 未连接时拨号 + 常驻登记 reservation 意图
                // （identify 后幂等建立），断连恢复与 reservation 重建一步到位
                let _ = endpoint
                    .add_infrastructure_peer(NodeAddr::with_addrs(peer, addrs), roles)
                    .await;
            });
        }
    }

    // === 后台收敛任务 ===

    /// infra 收敛后台任务：随 core 事件循环拉起，随 CancellationToken 结束。
    pub async fn run<TTransfer>(self: Arc<Self>, shared: crate::network::SharedNetRefs<TTransfer>)
    where
        TTransfer: Send + Sync + 'static,
    {
        let mut interval = n0_future::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(n0_future::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shared.cancel_token.cancelled() => break,
                _ = interval.tick() => self.tick(Instant::now()),
            }
        }
        tracing::info!("infra supervisor 退出");
    }
}

/// 过滤出对公网侧可用的直连地址（剔除私网/loopback/link-local/circuit）
fn usable_public_addrs(addrs: &[Addr]) -> Vec<Addr> {
    addrs
        .iter()
        .filter(|addr| addr.circuit_hops() == 0 && addr.is_public_routable())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::DiscoveryMode;
    use swarmdrop_net::{ProtocolId, SecretKey};

    async fn test_endpoint() -> Endpoint {
        Endpoint::builder()
            .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
            .bind()
            .await
            .expect("bind test endpoint")
    }

    async fn ctx(
        public_reachability: bool,
    ) -> (InfraSupervisor, Arc<RwLock<BootstrapCandidateManager>>) {
        let endpoint = test_endpoint().await;
        let candidates = Arc::new(RwLock::new(BootstrapCandidateManager::new(
            DiscoveryMode::Auto,
            true,
        )));
        let supervisor = InfraSupervisor::new(endpoint, candidates.clone(), public_reachability);
        (supervisor, candidates)
    }

    fn peer() -> NodeId {
        SecretKey::generate().node_id()
    }

    fn relay_candidate(
        candidates: &Arc<RwLock<BootstrapCandidateManager>>,
        scope: CandidateScope,
    ) -> NodeId {
        let p = peer();
        candidates.write().unwrap().upsert(
            p,
            vec!["/ip4/203.0.113.7/tcp/4001".parse().unwrap()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
            scope,
        );
        p
    }

    fn link_of(s: &InfraSupervisor, p: &NodeId) -> Option<RelayLinkState> {
        s.links.get(p).map(|e| *e.value())
    }

    fn identified(node: NodeId, agent: &str, addrs: Vec<Addr>) -> NetEvent {
        NetEvent::PeerIdentified {
            node,
            agent: agent.into(),
            protocol: "/swarmdrop/2.0.0".into(),
            addrs,
            protocols: vec![ProtocolId::from_static("/swarmdrop/pairing/2")],
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reservation_lost_schedules_rebuild_with_backoff() {
        let (s, candidates) = ctx(true).await;
        let p = relay_candidate(&candidates, CandidateScope::Public);

        s.handle_event(&NetEvent::RelayReservationAccepted {
            relay: p,
            renewal: false,
        });
        assert!(link_of(&s, &p).unwrap().reservation_active);

        s.handle_event(&NetEvent::RelayReservationLost { relay: p });
        let link = link_of(&s, &p).unwrap();
        assert!(!link.reservation_active);

        // tick 触发首轮重建并进入退避
        let now = Instant::now();
        s.tick(now);
        let link = link_of(&s, &p).unwrap();
        assert_eq!(link.attempts, 1);
        assert!(link.next_attempt_at > now);

        // 退避期内不重复尝试
        s.tick(now + Duration::from_millis(500));
        assert_eq!(link_of(&s, &p).unwrap().attempts, 1);

        // 退避到期后再次尝试，间隔递增
        s.tick(now + Duration::from_secs(3));
        let link = link_of(&s, &p).unwrap();
        assert_eq!(link.attempts, 2);

        // 回归：重试期间每次失败都会再来一条 Lost（如被 relay 拒绝），
        // 不得清零退避进度，否则退化成 1-2s 重试风暴
        s.handle_event(&NetEvent::RelayReservationLost { relay: p });
        let link = link_of(&s, &p).unwrap();
        assert_eq!(link.attempts, 2, "Lost 不得重置 attempts");
        assert!(
            link.next_attempt_at > now + Duration::from_secs(3),
            "Lost 不得提前 next_attempt_at"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn removed_candidate_is_not_converged() {
        let (s, candidates) = ctx(true).await;
        let p = relay_candidate(&candidates, CandidateScope::Public);

        // 先经历一轮收敛（links 有状态）
        s.tick(Instant::now());
        assert!(link_of(&s, &p).is_some());

        // 注销：候选 + links 双清（NetManager::remove_relay_intent 的顺序）
        candidates.write().unwrap().remove(p);
        s.remove(p);
        assert!(link_of(&s, &p).is_none());

        // 后续 tick 不再对它有任何收敛动作
        s.tick(Instant::now() + Duration::from_secs(10));
        assert!(link_of(&s, &p).is_none(), "注销后 tick 不得复活 links");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reverse_convergence_removes_orphan_registration() {
        let (s, _candidates) = ctx(true).await;
        let ghost = peer();

        // 制造孤儿登记：内核有 relay 意图（watch_relays 出条目）而候选表无该 peer
        // ——等价于「注销与在途注册任务竞态、登记被复活」的终局形态
        s.endpoint
            .ensure_relay_reservation(swarmdrop_net::NodeAddr::with_addrs(
                ghost,
                vec!["/ip4/127.0.0.1/tcp/1".parse().unwrap()],
            ))
            .await
            .expect("register orphan intent");
        let mut watcher = s.endpoint.watch_relays();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !watcher.get().contains_key(&ghost) {
                watcher.updated().await.expect("watch closed");
            }
        })
        .await
        .expect("orphan entry should appear");

        // 一轮 tick：差集发现「内核有、候选无」→ 幂等注销（spawn 异步完成）
        s.tick(Instant::now());
        tokio::time::timeout(Duration::from_secs(5), async {
            while watcher.get().contains_key(&ghost) {
                watcher.updated().await.expect("watch closed");
            }
        })
        .await
        .expect("reverse convergence should remove orphan entry");
        assert!(link_of(&s, &ghost).is_none(), "links 一并清除");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn removed_candidate_can_be_relearned() {
        let (s, candidates) = ctx(true).await;
        let boot = peer();
        let addr: Addr = "/ip4/47.115.172.218/tcp/4001".parse().unwrap();

        s.handle_event(&identified(
            boot,
            "swarm-bootstrap/0.4.1",
            vec![addr.clone()],
        ));
        assert!(candidates.read().unwrap().get(boot).is_some());

        candidates.write().unwrap().remove(boot);
        s.remove(boot);

        // 该节点后续真实可达并重新宣告：学习型候选重新纳管是正确行为
        s.handle_event(&identified(boot, "swarm-bootstrap/0.4.1", vec![addr]));
        assert!(
            candidates.read().unwrap().get(boot).is_some(),
            "重新 identify 后应可重新纳管"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn public_reachability_off_skips_public_candidates() {
        let (s, candidates) = ctx(false).await;
        let public_peer = relay_candidate(&candidates, CandidateScope::Public);
        let lan_peer = relay_candidate(&candidates, CandidateScope::Lan);

        s.tick(Instant::now());

        assert!(
            link_of(&s, &public_peer).is_none(),
            "public_reachability=false 时不得对 Public 候选做 reservation"
        );
        assert!(
            link_of(&s, &lan_peer).is_some(),
            "LAN 候选不受 public_reachability 约束"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn learns_bootstrap_agent_as_candidate() {
        let (s, candidates) = ctx(true).await;
        let boot = peer();

        s.handle_event(&identified(
            boot,
            "swarm-bootstrap/0.4.1",
            vec![
                "/ip4/47.115.172.218/tcp/4001".parse().unwrap(),
                "/ip4/127.0.0.1/tcp/4001".parse().unwrap(),
                "/ip4/192.168.1.5/tcp/4001".parse().unwrap(),
            ],
        ));

        let candidate = candidates.read().unwrap().get(boot).expect("learned");
        assert!(
            candidate
                .sources
                .contains(&BootstrapCandidateSource::Learned)
        );
        assert_eq!(candidate.scope, CandidateScope::Public);
        // 只保留公网可用地址
        assert_eq!(candidate.addrs.len(), 1);
        assert!(candidate.addrs[0].to_string().contains("47.115.172.218"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_bootstrap_agent_is_not_learned() {
        let (s, candidates) = ctx(true).await;
        let p = peer();

        s.handle_event(&identified(
            p,
            "swarmdrop/0.7.6 (macos)",
            vec!["/ip4/47.115.172.218/tcp/4001".parse().unwrap()],
        ));

        assert!(candidates.read().unwrap().get(p).is_none());
    }
}
