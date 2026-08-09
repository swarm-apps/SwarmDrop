use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, InfraRoles, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BootstrapCandidateSource {
    /// 当前 host 注入的静态引导/中继配置（含各端默认值和用户追加地址）。
    HostConfigured,
    MdnsLanHelper,
    /// 运行时经 identify 学到的基础设施节点（如经局域网协助节点认识的公网中继）
    Learned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CandidateScope {
    Public,
    Lan,
}

impl CandidateScope {
    /// 该地址是否让持有者成为**公网可达的基础设施**：非 circuit 的公网可路由地址。
    ///
    /// 判据的**唯一定义处**——`infra::supervisor::usable_public_addrs`（决定学到哪些
    /// 地址）与 [`infer`](Self::infer)（决定据此判什么 scope）共用它。两边各写一份时，
    /// 「收进表的地址」与「按表算出的 scope」会对不上。
    ///
    /// circuit 地址排除在外：经中继才够得着的节点自己不是公网入口，也不可能在它上面
    /// 建 reservation。
    pub fn is_infra_public_addr(addr: &Addr) -> bool {
        addr.circuit_hops() == 0 && addr.is_public_routable()
    }

    /// 从地址形状推断 scope（HostConfigured 等无来源先验的候选用）。
    ///
    /// **判据是「持有公网地址」，不是「不含私网地址」。** 两者只在混合地址候选上分歧，
    /// 而那正是要修的形态：`upsert` 按合并后的全部地址重算 scope、地址表又只增不减，
    /// 于是旧判据下一台真·公网中继只要有一条私网地址进过表（自建 bootstrap 跑在同一
    /// 局域网、用户按内网地址把它加进来，随后 identify 并入它的公网地址），scope 就
    /// 永久停在 `Lan` 再也回不去——`exclusion_for` 的闸门恒假，关掉「公网可达性」的
    /// 用户照样在一台公网中继上建了 reservation，被跨网直达。
    ///
    /// 翻过来之后 `Public` 成了吸收态，方向是安全的那一侧：地址只增不减，「见过公网
    /// 地址」这个事实本来就不该被后来的私网地址抹掉。
    ///
    /// 纯局域网 helper（只有私网/loopback 地址）仍判 `Lan`、仍不受公网开关约束——
    /// 用户手动点名的本地 helper 不应被公网开关拦下，这条原意保留。
    pub fn infer(addrs: &[Addr]) -> Self {
        if addrs.iter().any(Self::is_infra_public_addr) {
            Self::Public
        } else {
            Self::Lan
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CandidateRoles {
    pub kad_server: bool,
    pub relay_server: bool,
}

impl CandidateRoles {
    pub fn kad_and_relay() -> Self {
        Self {
            kad_server: true,
            relay_server: true,
        }
    }
}

impl From<CandidateRoles> for InfraRoles {
    fn from(value: CandidateRoles) -> Self {
        Self {
            kad_server: value.kad_server,
            relay: value.relay_server,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BootstrapCandidate {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: NodeId,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub addrs: Vec<Addr>,
    pub sources: Vec<BootstrapCandidateSource>,
    pub roles: CandidateRoles,
    /// 由 [`BootstrapCandidateManager::upsert`] 按全部地址单点推断，调用方不得指定。
    pub scope: CandidateScope,
    /// 首次登记时刻，此后不可变——宽限期状态机的时间锚。
    ///
    /// 与 `last_seen` 的分工：后者每次 upsert 刷新（用于重置退避），所以回答不了
    /// 「这条候选存在多久了」；被 mDNS 反复重新发现的 helper 会让它永远是「刚刚」。
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CandidateSourceStatus {
    pub source: BootstrapCandidateSource,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct BootstrapCandidateManager {
    auto_discover_lan_helpers: bool,
    candidates: HashMap<NodeId, BootstrapCandidate>,
}

impl BootstrapCandidateManager {
    pub fn new(auto_discover_lan_helpers: bool) -> Self {
        Self {
            auto_discover_lan_helpers,
            candidates: HashMap::new(),
        }
    }

    pub fn auto_discover_lan_helpers(&self) -> bool {
        self.auto_discover_lan_helpers
    }

    /// 登记或合并一个候选。
    ///
    /// **`scope` 不接受参数**——由本函数按合并后的**全部**地址 [`CandidateScope::infer`]
    /// 计算。此前它由调用方传入，而三个调用点给了三种拼法（启动路径硬编码 `Public`、
    /// 运行时意图用 `infer`、局域网协助路径硬编码 `Lan`），加上这里对 scope 是**直接
    /// 覆盖**而对 roles 是累加，于是一个既被用户手填（含私网地址）又被 identify 认出的
    /// 节点，scope 会在 `Lan`/`Public` 之间来回翻转——而 `wants_reservation` 直接吃它，
    /// 该候选就在收敛环里时进时出。收进来算一次，三种拼法与覆盖翻转一起消失。
    pub fn upsert(
        &mut self,
        peer_id: NodeId,
        addrs: Vec<Addr>,
        source: BootstrapCandidateSource,
        roles: CandidateRoles,
    ) -> bool {
        if addrs.is_empty() {
            return false;
        }

        let now = Utc::now();
        match self.candidates.get_mut(&peer_id) {
            Some(candidate) => {
                let mut changed = false;
                for addr in addrs {
                    if !candidate.addrs.contains(&addr) {
                        candidate.addrs.push(addr);
                        changed = true;
                    }
                }
                if !candidate.sources.contains(&source) {
                    candidate.sources.push(source);
                    changed = true;
                }
                candidate.roles.kad_server |= roles.kad_server;
                candidate.roles.relay_server |= roles.relay_server;
                // 按合并后的全部地址重算：新地址可能带来这个候选的第一条公网地址。
                candidate.scope = CandidateScope::infer(&candidate.addrs);
                candidate.last_seen = now;
                changed
            }
            None => {
                let scope = CandidateScope::infer(&addrs);
                self.candidates.insert(
                    peer_id,
                    BootstrapCandidate {
                        peer_id,
                        addrs,
                        sources: vec![source],
                        roles,
                        scope,
                        // 宽限期状态机的时间锚：首次登记时刻，此后不可变。
                        // `last_seen` 会被重新发现刷新，当不了「这条存在多久了」的基准。
                        first_seen: now,
                        last_seen: now,
                    },
                );
                true
            }
        }
    }

    /// 移除候选（注销基础设施节点的策略层清理）。
    pub fn remove(&mut self, peer_id: NodeId) {
        self.candidates.remove(&peer_id);
    }

    pub fn get(&self, peer_id: NodeId) -> Option<BootstrapCandidate> {
        self.candidates.get(&peer_id).cloned()
    }

    /// 全量候选快照（infra 收敛层 tick 时消费）
    pub fn snapshot(&self) -> Vec<BootstrapCandidate> {
        self.candidates.values().cloned().collect()
    }

    pub fn contains(&self, peer_id: NodeId) -> bool {
        self.candidates.contains_key(&peer_id)
    }

    /// Learned 来源候选数量（学习型候选上限约束用）
    pub fn learned_count(&self) -> usize {
        self.candidates
            .values()
            .filter(|c| c.sources.contains(&BootstrapCandidateSource::Learned))
            .count()
    }

    pub fn lan_helper_count(&self) -> usize {
        self.candidates
            .values()
            .filter(|candidate| {
                candidate
                    .sources
                    .contains(&BootstrapCandidateSource::MdnsLanHelper)
            })
            .count()
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn source_statuses(&self) -> Vec<CandidateSourceStatus> {
        let mut counts: HashMap<BootstrapCandidateSource, usize> = HashMap::new();
        for candidate in self.candidates.values() {
            for source in &candidate.sources {
                *counts.entry(*source).or_default() += 1;
            }
        }
        let mut statuses: Vec<_> = counts
            .into_iter()
            .map(|(source, count)| CandidateSourceStatus { source, count })
            .collect();
        statuses.sort_by_key(|status| match status.source {
            BootstrapCandidateSource::HostConfigured => 0,
            BootstrapCandidateSource::MdnsLanHelper => 1,
            BootstrapCandidateSource::Learned => 3,
        });
        statuses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarmdrop_net::SecretKey;

    fn peer_id() -> NodeId {
        SecretKey::generate().node_id()
    }

    #[test]
    fn upsert_merges_sources_and_addresses() {
        let peer = peer_id();
        let addr1: Addr = "/ip4/192.168.1.2/tcp/4001".parse().unwrap();
        let addr2: Addr = "/ip4/192.168.1.3/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(true);

        assert!(manager.upsert(
            peer,
            vec![addr1.clone()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        ));
        assert!(manager.upsert(
            peer,
            vec![addr1.clone(), addr2.clone()],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
        ));

        let candidate = manager.get(peer).unwrap();
        assert_eq!(candidate.addrs, vec![addr1, addr2]);
        assert_eq!(candidate.sources.len(), 2);
        assert_eq!(manager.lan_helper_count(), 1);
    }

    /// scope 由候选表按**合并后**的全部地址单点推断，且 `Public` 一旦成立不再翻回。
    ///
    /// 回归两个形态：
    /// ① 调用方各传各的 scope（启动路径硬编码 `Public`、局域网协助路径硬编码 `Lan`），
    ///    而 upsert 对 scope 是直接覆盖——同一候选被两条路径先后登记就来回翻；
    /// ② 判据写成「任一私网地址即 Lan」时，一台真·公网中继只要有条私网地址进过表，
    ///    scope 就永久停在 `Lan`，`exclusion_for` 的公网闸门对它恒假。
    #[test]
    fn scope_stays_public_once_a_public_addr_is_known() {
        let peer = peer_id();
        let lan: Addr = "/ip4/192.168.1.2/tcp/4001".parse().unwrap();
        let public: Addr = "/ip4/203.0.113.7/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(true);

        // 先按内网地址手填（自建 bootstrap 跑在同一局域网的典型形态）→ Lan
        manager.upsert(
            peer,
            vec![lan.clone()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(manager.get(peer).unwrap().scope, CandidateScope::Lan);

        // identify 并入它的公网地址 → 它确实是台公网中继，必须翻 Public 才受闸门约束
        manager.upsert(
            peer,
            vec![public],
            BootstrapCandidateSource::Learned,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(manager.get(peer).unwrap().scope, CandidateScope::Public);

        // 又学到一条私网地址 → 仍是 Public，不得被私网地址抹掉
        manager.upsert(
            peer,
            vec!["/ip4/192.168.1.3/tcp/4001".parse().unwrap()],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(
            manager.get(peer).unwrap().scope,
            CandidateScope::Public,
            "已知持有公网地址的候选，scope 不得被后来的私网地址翻回 Lan"
        );
    }

    /// circuit 地址不算公网入口——经中继才够得着的节点不可能在它上面建 reservation。
    #[test]
    fn a_circuit_only_candidate_is_not_public() {
        let peer = peer_id();
        let circuit: Addr =
            "/ip4/203.0.113.7/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX/p2p-circuit"
                .parse()
                .unwrap();
        let mut manager = BootstrapCandidateManager::new(true);

        manager.upsert(
            peer,
            vec![circuit],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        );

        assert_eq!(manager.get(peer).unwrap().scope, CandidateScope::Lan);
    }

    /// 角色累加、scope 重算：两条路径各给一半角色时不能互相覆盖。
    #[test]
    fn roles_accumulate_across_upserts() {
        let peer = peer_id();
        let addr: Addr = "/ip4/203.0.113.7/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(true);

        manager.upsert(
            peer,
            vec![addr.clone()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles {
                kad_server: true,
                relay_server: false,
            },
        );
        manager.upsert(
            peer,
            vec![addr],
            BootstrapCandidateSource::Learned,
            CandidateRoles {
                kad_server: false,
                relay_server: true,
            },
        );

        let roles = manager.get(peer).unwrap().roles;
        assert!(roles.kad_server && roles.relay_server, "角色应累加而非覆盖");
    }

    /// 重新发现刷新 `last_seen`（用于重置退避），但不动 `first_seen`。
    ///
    /// `first_seen` 是宽限期状态机的时间锚：被 mDNS 反复重新发现的 helper 若让它
    /// 跟着刷新，「这条存在多久了」就永远是「刚刚」，宽限期永不到期。
    #[test]
    fn rediscovery_refreshes_last_seen_but_not_first_seen() {
        let peer = peer_id();
        let addr1: Addr = "/ip4/192.168.1.2/tcp/4001".parse().unwrap();
        let addr2: Addr = "/ip4/192.168.1.3/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(true);

        manager.upsert(
            peer,
            vec![addr1],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
        );
        let first = manager.get(peer).unwrap();

        manager.upsert(
            peer,
            vec![addr2],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
        );
        let second = manager.get(peer).unwrap();

        assert_eq!(second.first_seen, first.first_seen, "first_seen 不可变");
        assert!(second.last_seen >= first.last_seen);
    }
}
