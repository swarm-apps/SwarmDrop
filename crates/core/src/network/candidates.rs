use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use swarm_p2p_core::InfrastructureRoles;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};

use super::DiscoveryMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BootstrapCandidateSource {
    BuiltInPublic,
    UserCustom,
    MdnsLanHelper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CandidateScope {
    Public,
    Lan,
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

impl From<CandidateRoles> for InfrastructureRoles {
    fn from(value: CandidateRoles) -> Self {
        Self {
            kad_server: value.kad_server,
            relay_server: value.relay_server,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BootstrapCandidate {
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: PeerId,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub addrs: Vec<Multiaddr>,
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
    candidates: HashMap<PeerId, BootstrapCandidate>,
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
        peer_id: PeerId,
        addrs: Vec<Multiaddr>,
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

    pub fn mark_connected(&mut self, peer_id: PeerId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id)
            && !matches!(candidate.health, CandidateHealth::RelayReady)
        {
            candidate.health = CandidateHealth::Connected;
        }
    }

    pub fn mark_relay_ready(&mut self, peer_id: PeerId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id) {
            candidate.health = CandidateHealth::RelayReady;
        }
    }

    pub fn mark_failed(&mut self, peer_id: PeerId) {
        if let Some(candidate) = self.candidates.get_mut(&peer_id) {
            candidate.health = CandidateHealth::Failed;
        }
    }

    pub fn get(&self, peer_id: PeerId) -> Option<BootstrapCandidate> {
        self.candidates.get(&peer_id).cloned()
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
        });
        statuses
    }

    pub fn relay_source(&self, peer_id: PeerId) -> Option<BootstrapCandidateSource> {
        self.candidates
            .get(&peer_id)
            .and_then(|candidate| candidate.sources.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarm_p2p_core::libp2p::identity::Keypair;

    fn peer_id() -> PeerId {
        Keypair::generate_ed25519().public().to_peer_id()
    }

    #[test]
    fn upsert_merges_sources_and_addresses() {
        let peer = peer_id();
        let addr1: Multiaddr = "/ip4/192.168.1.2/tcp/4001".parse().unwrap();
        let addr2: Multiaddr = "/ip4/192.168.1.3/tcp/4001".parse().unwrap();
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
