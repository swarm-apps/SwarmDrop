//! 基础设施链路的收敛状态机。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use swarm_p2p_core::NodeEvent;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use tokio::time::Instant;

use crate::device::OsInfo;
use crate::network::candidates::{
    BootstrapCandidate, BootstrapCandidateManager, BootstrapCandidateSource, CandidateRoles,
    CandidateScope,
};
use crate::protocol::{AppNetClient, AppRequest};

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
    client: AppNetClient,
    candidates: Arc<RwLock<BootstrapCandidateManager>>,
    /// 公网可达性设置：false 时不对 Public 范围候选做 reservation
    public_reachability: bool,
    /// relay 候选的收敛状态（key = 候选 peer）
    links: DashMap<PeerId, RelayLinkState>,
}

impl InfraSupervisor {
    pub fn new(
        client: AppNetClient,
        candidates: Arc<RwLock<BootstrapCandidateManager>>,
        public_reachability: bool,
    ) -> Self {
        Self {
            client,
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

    // === 事件折叠（core 事件循环调用） ===

    pub fn handle_event(&self, event: &NodeEvent<AppRequest>) {
        match event {
            NodeEvent::RelayReservationAccepted { relay_peer_id, .. } => {
                self.links.insert(
                    *relay_peer_id,
                    RelayLinkState {
                        reservation_active: true,
                        next_attempt_at: Instant::now(),
                        attempts: 0,
                        candidate_seen: chrono::Utc::now(),
                    },
                );
            }
            NodeEvent::RelayReservationLost { relay_peer_id } => {
                tracing::info!("relay reservation 丢失: {relay_peer_id}，进入重建");
                // 只翻可用位，保留既有退避进度——reservation 被 relay 拒绝时
                // 每次尝试都会产生一次 Lost，无条件清零会退化成 1-2s 重试风暴。
                // attempts 仅由 Accepted（健康恢复）与候选 last_seen 刷新（重新发现）归零。
                self.links
                    .entry(*relay_peer_id)
                    .and_modify(|link| link.reservation_active = false)
                    .or_insert(RelayLinkState {
                        reservation_active: false,
                        next_attempt_at: Instant::now(),
                        attempts: 0,
                        candidate_seen: chrono::Utc::now(),
                    });
            }
            // 学习型候选：识别基础设施 agent 自动纳管
            NodeEvent::IdentifyReceived {
                peer_id,
                agent_version,
                listen_addrs,
                ..
            } if OsInfo::is_bootstrap_agent(agent_version) => {
                self.learn_candidate(*peer_id, listen_addrs);
            }
            _ => {}
        }
    }

    /// 把运行时认识的基础设施节点纳入候选表（Learned 来源）。
    fn learn_candidate(&self, peer_id: PeerId, listen_addrs: &[Multiaddr]) {
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
            let client = self.client.clone();
            tokio::spawn(async move {
                let _ = client
                    .add_infrastructure_peer(
                        peer_id,
                        addrs,
                        swarm_p2p_core::InfrastructureRoles::kad_server(),
                    )
                    .await;
            });
        }
    }

    // === 收敛推进（定时任务调用） ===

    /// 对每个应持有 reservation 的候选做一轮收敛检查。
    pub fn tick(&self, now: Instant) {
        let snapshot = self
            .candidates
            .read()
            .map(|c| c.snapshot())
            .unwrap_or_default();

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

            let client = self.client.clone();
            let peer = candidate.peer_id;
            let addrs = candidate.addrs.clone();
            let roles = candidate.roles.into();
            tokio::spawn(async move {
                tracing::debug!("收敛基础设施链路: {peer}（第 {attempts} 次尝试）");
                // 全角色注册：kad 重接线 + 未连接时拨号 + 常驻登记 reservation 意图
                // （identify 后幂等建立），断连恢复与 reservation 重建一步到位
                let _ = client.add_infrastructure_peer(peer, addrs, roles).await;
            });
        }
    }

    // === 后台收敛任务 ===

    /// infra 收敛后台任务：随 core 事件循环拉起，随 CancellationToken 结束。
    pub async fn run<TTransfer>(self: Arc<Self>, shared: crate::network::SharedNetRefs<TTransfer>)
    where
        TTransfer: Send + Sync + 'static,
    {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
fn usable_public_addrs(addrs: &[Multiaddr]) -> Vec<Multiaddr> {
    use swarm_p2p_core::addr::{circuit_hops, is_public_routable};
    addrs
        .iter()
        .filter(|addr| circuit_hops(addr) == 0 && is_public_routable(addr))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::DiscoveryMode;
    use crate::protocol::{AppRequest as Req, AppResponse as Resp};
    use swarm_p2p_core::libp2p::identity::Keypair;

    fn test_client() -> (AppNetClient, swarm_p2p_core::EventReceiver<Req>) {
        let keypair = Keypair::generate_ed25519();
        let config = swarm_p2p_core::NodeConfig::new("/swarmdrop-infra-test/1.0.0", "test/1.0.0")
            .with_listen_addrs(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
            .with_mdns(false)
            .with_relay_client(true)
            .with_dcutr(false)
            .with_autonat(false);
        let (client, events, _dc) =
            swarm_p2p_core::start::<Req, Resp>(keypair, config).expect("start test node");
        (client, events)
    }

    fn ctx(
        public_reachability: bool,
    ) -> (
        InfraSupervisor,
        Arc<RwLock<BootstrapCandidateManager>>,
        swarm_p2p_core::EventReceiver<Req>,
    ) {
        let (client, events) = test_client();
        let candidates = Arc::new(RwLock::new(BootstrapCandidateManager::new(
            DiscoveryMode::Auto,
            true,
        )));
        let supervisor = InfraSupervisor::new(client, candidates.clone(), public_reachability);
        (supervisor, candidates, events)
    }

    fn peer() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    fn relay_candidate(
        candidates: &Arc<RwLock<BootstrapCandidateManager>>,
        scope: CandidateScope,
    ) -> PeerId {
        let p = peer();
        candidates.write().unwrap().upsert(
            p,
            vec!["/ip4/203.0.113.7/tcp/4001".parse().unwrap()],
            BootstrapCandidateSource::BuiltInPublic,
            CandidateRoles::kad_and_relay(),
            scope,
        );
        p
    }

    fn link_of(s: &InfraSupervisor, p: &PeerId) -> Option<RelayLinkState> {
        s.links.get(p).map(|e| *e.value())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reservation_lost_schedules_rebuild_with_backoff() {
        let (s, candidates, _ev) = ctx(true);
        let p = relay_candidate(&candidates, CandidateScope::Public);

        s.handle_event(&NodeEvent::RelayReservationAccepted {
            relay_peer_id: p,
            renewal: false,
        });
        assert!(link_of(&s, &p).unwrap().reservation_active);

        s.handle_event(&NodeEvent::RelayReservationLost { relay_peer_id: p });
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
        s.handle_event(&NodeEvent::RelayReservationLost { relay_peer_id: p });
        let link = link_of(&s, &p).unwrap();
        assert_eq!(link.attempts, 2, "Lost 不得重置 attempts");
        assert!(
            link.next_attempt_at > now + Duration::from_secs(3),
            "Lost 不得提前 next_attempt_at"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn public_reachability_off_skips_public_candidates() {
        let (s, candidates, _ev) = ctx(false);
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
        let (s, candidates, _ev) = ctx(true);
        let boot = peer();

        s.handle_event(&NodeEvent::IdentifyReceived {
            peer_id: boot,
            agent_version: "swarm-bootstrap/0.4.1".into(),
            protocol_version: "/swarmdrop/1.0.0".into(),
            listen_addrs: vec![
                "/ip4/47.115.172.218/tcp/4001".parse().unwrap(),
                "/ip4/127.0.0.1/tcp/4001".parse().unwrap(),
                "/ip4/192.168.1.5/tcp/4001".parse().unwrap(),
            ],
            protocols: vec![],
        });

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
        let (s, candidates, _ev) = ctx(true);
        let p = peer();

        s.handle_event(&NodeEvent::IdentifyReceived {
            peer_id: p,
            agent_version: "swarmdrop/0.7.6 (macos)".into(),
            protocol_version: "/swarmdrop/1.0.0".into(),
            listen_addrs: vec!["/ip4/47.115.172.218/tcp/4001".parse().unwrap()],
            protocols: vec![],
        });

        assert!(candidates.read().unwrap().get(p).is_none());
    }
}
