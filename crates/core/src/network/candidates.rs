use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use swarmdrop_net::{Addr, InfraRoles, NodeId};

use super::DiscoveryMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BootstrapCandidateSource {
    BuiltInPublic,
    UserCustom,
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
    /// 从地址形状推断 scope（UserCustom 等无来源先验的候选用）。
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CandidateHealth {
    Unknown,
    Connected,
    RelayReady,
    Failed,
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
    pub scope: CandidateScope,
    pub last_seen: DateTime<Utc>,
    pub health: CandidateHealth,
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

    pub fn upsert(
        &mut self,
        peer_id: NodeId,
        addrs: Vec<Addr>,
        source: BootstrapCandidateSource,
        roles: CandidateRoles,
        scope: CandidateScope,
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
                candidate.scope = scope;
                candidate.last_seen = now;
                changed
            }
            None => {
                self.candidates.insert(
                    peer_id,
                    BootstrapCandidate {
                        peer_id,
                        addrs,
                        sources: vec![source],
                        roles,
                        scope,
                        last_seen: now,
                        health: CandidateHealth::Unknown,
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

    pub fn mark_connected(&mut self, peer_id: NodeId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id)
            && !matches!(candidate.health, CandidateHealth::RelayReady)
        {
            candidate.health = CandidateHealth::Connected;
        }
    }

    pub fn mark_relay_ready(&mut self, peer_id: NodeId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id) {
            candidate.health = CandidateHealth::RelayReady;
        }
    }

    pub fn mark_failed(&mut self, peer_id: NodeId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id) {
            candidate.health = CandidateHealth::Failed;
        }
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
            BootstrapCandidateSource::UserCustom => 0,
            BootstrapCandidateSource::MdnsLanHelper => 1,
            BootstrapCandidateSource::BuiltInPublic => 2,
            BootstrapCandidateSource::Learned => 3,
        });
        statuses
    }

    pub fn relay_source(&self, peer_id: NodeId) -> Option<BootstrapCandidateSource> {
        self.candidates
            .get(&peer_id)
            .and_then(|candidate| candidate.sources.first().copied())
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
            BootstrapCandidateSource::BuiltInPublic,
            CandidateRoles::kad_and_relay(),
            CandidateScope::Public,
        ));
        assert!(manager.upsert(
            peer,
            vec![addr1.clone(), addr2.clone()],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
            CandidateScope::Lan,
        ));

        let candidate = manager.get(peer).unwrap();
        assert_eq!(candidate.addrs, vec![addr1, addr2]);
        assert_eq!(candidate.sources.len(), 2);
        assert_eq!(manager.lan_helper_count(), 1);
    }
}
