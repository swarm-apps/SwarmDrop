use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, InfraRoles, NodeId};

use super::DiscoveryMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BootstrapCandidateSource {
    /// 当前 host 注入的静态引导/中继配置（含各端默认值和用户追加地址）。
    HostConfigured,
    MdnsLanHelper,
    /// 运行时经 identify 学到的基础设施节点（如 LanOnly 下经 LAN Helper 认识的公网中继）
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
    /// 从地址形状推断 scope（HostConfigured 等无来源先验的候选用）。
    ///
    /// 任一私网/loopback 地址即判 Lan——注意这意味着混合地址候选会**绕过
    /// `public_reachability` 闸门**（supervisor 对 Lan 候选无条件收敛），
    /// 这是有意的：用户手动点名的本地 helper 不应被公网开关拦下。
    pub fn infer(addrs: &[Addr]) -> Self {
        if addrs.iter().any(|a| a.is_private_lan() || a.is_loopback()) {
            Self::Lan
        } else {
            Self::Public
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
    discovery_mode: DiscoveryMode,
    auto_discover_lan_helpers: bool,
    candidates: HashMap<NodeId, BootstrapCandidate>,
}

impl BootstrapCandidateManager {
    pub fn new(discovery_mode: DiscoveryMode, auto_discover_lan_helpers: bool) -> Self {
        Self {
            discovery_mode,
            auto_discover_lan_helpers,
            candidates: HashMap::new(),
        }
    }

    pub fn discovery_mode(&self) -> DiscoveryMode {
        self.discovery_mode
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
                // 按合并后的全部地址重算：新地址可能把一个原判 Public 的候选带进 LAN 范围。
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
        let mut manager = BootstrapCandidateManager::new(DiscoveryMode::Auto, true);

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

    /// scope 由候选表按**合并后**的全部地址单点推断，二次 upsert 不会把它翻回去。
    ///
    /// 回归的是这个形态：调用方各传各的 scope（启动路径硬编码 `Public`、局域网协助
    /// 路径硬编码 `Lan`），而 upsert 对 scope 是直接覆盖——同一个候选被两条路径先后
    /// 登记就会来回翻，`wants_reservation` 直接吃它，收敛环时进时出。
    #[test]
    fn scope_is_inferred_from_all_addrs_and_never_flips_back() {
        let peer = peer_id();
        let lan: Addr = "/ip4/192.168.1.2/tcp/4001".parse().unwrap();
        let public: Addr = "/ip4/203.0.113.7/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(DiscoveryMode::Auto, true);

        // 先以公网地址登记 → Public
        manager.upsert(
            peer,
            vec![public.clone()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(manager.get(peer).unwrap().scope, CandidateScope::Public);

        // 再经 identify 学到一条私网地址 → 合并后含私网，判 Lan
        manager.upsert(
            peer,
            vec![lan],
            BootstrapCandidateSource::Learned,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(manager.get(peer).unwrap().scope, CandidateScope::Lan);

        // 同一条公网地址再来一次（重复 upsert 不新增地址）→ 仍是 Lan，不翻回 Public
        manager.upsert(
            peer,
            vec![public],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        );
        assert_eq!(
            manager.get(peer).unwrap().scope,
            CandidateScope::Lan,
            "合并后仍含私网地址，scope 不得翻回 Public"
        );
    }

    /// 角色累加、scope 重算：两条路径各给一半角色时不能互相覆盖。
    #[test]
    fn roles_accumulate_across_upserts() {
        let peer = peer_id();
        let addr: Addr = "/ip4/203.0.113.7/tcp/4001".parse().unwrap();
        let mut manager = BootstrapCandidateManager::new(DiscoveryMode::Auto, true);

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
        let mut manager = BootstrapCandidateManager::new(DiscoveryMode::Auto, true);

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
