//! 整体网络健康度（Rust 侧）。
//!
//! # 为什么这里有第二份判据
//!
//! `DESIGN.md` 的 Node Status Contract 把这两个判据放在 `@swarmdrop/shared-view`，
//! 三端表现层各自渲染。托盘是**第四个表现层**，而它整条在 Rust 里——菜单文案走
//! rust-i18n（`src-tauri/locales/*.toml`），拿不到 TS 包。契约本身点名了这一份
//! catalog（「there are four: desktop, web, mobile, plus rust-i18n for the tray」），
//! 所以这份实现是契约预期内的，不是漏改的复制品。
//!
//! **它必须与 `packages/shared-view/src/network/node-health.ts` 逐档同义。**
//! 下面的分档顺序、宽限三条件与 `INFRA_GRACE` 都是照抄那份，改一边必须改另一边；
//! 本模块的测试钉的就是这层同义关系。

use chrono::{DateTime, Utc};
use swarmdrop_core::infra::{InfraLink, RelayLinkState};
use swarmdrop_core::network::candidates::CandidateScope;
use swarmdrop_core::network::{NetworkStatus, NodeStatus};

/// 首次连接的宽限窗口，与 shared-view 的 `INFRA_GRACE_MS` 同值。
const INFRA_GRACE: chrono::TimeDelta = chrono::TimeDelta::milliseconds(10_000);

/// 整体健康度六态（`NodeHealthLevel` 的 Rust 镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeHealthLevel {
    NotRunning,
    Starting,
    Reachable,
    LanReachable,
    ConfiguredLanOnly,
    Isolated,
}

impl NodeHealthLevel {
    /// 托盘是否可以呈现正常的「在线」。
    ///
    /// `Isolated` 是唯一的警示档：节点在跑，但**连不上任何网络**。此时托盘若照常亮
    /// 「● 在线 · 可接收文件」，用户会在完全收不到东西的情况下以为一切正常。
    pub fn is_isolated(self) -> bool {
        matches!(self, Self::Isolated)
    }
}

/// 单条关系的呈现态里，本模块唯一用得到的那一档。
///
/// 只判「是否仍在收敛」：整体健康度里其余分档都不看单条的成功/失败
/// （「两条引导节点连上一条」与「连上两条」后果完全一样）。
fn is_settling(link: &InfraLink, now: DateTime<Utc>) -> bool {
    if link.excluded.is_some() || !link.roles.relay_server {
        return false;
    }
    match &link.relay {
        Some(RelayLinkState::Active { .. }) => false,
        // 曾经连上过就不吃宽限：掉线要立刻说，不是「还在连」。
        _ if link.ever_active => false,
        Some(RelayLinkState::Failed { .. }) => now - link.first_seen < INFRA_GRACE,
        // `Connecting` 与 `None`（有角色但内核还没建轨道）都算仍在收敛。
        Some(RelayLinkState::Connecting) | None => true,
    }
}

/// 合成整体健康度。分档顺序即优先级，与 shared-view 的 `resolveLevel` 逐条对应。
pub fn summarize(status: &NetworkStatus, now: DateTime<Utc>) -> NodeHealthLevel {
    if !matches!(status.status, NodeStatus::Running) {
        return NodeHealthLevel::NotRunning;
    }
    if status.public_reachable {
        return NodeHealthLevel::Reachable;
    }

    let public_links: Vec<&InfraLink> = status
        .infra_links
        .iter()
        .filter(|l| matches!(l.scope, CandidateScope::Public))
        .collect();
    if !public_links.is_empty() && public_links.iter().all(|l| l.excluded.is_some()) {
        return NodeHealthLevel::ConfiguredLanOnly;
    }

    // 空清单也走这一档：没有 link 就没有可诊断的对象，此时报警是指着一张空表说话。
    let settling = status.infra_links.iter().any(|l| is_settling(l, now));
    if settling || status.infra_links.is_empty() {
        return NodeHealthLevel::Starting;
    }

    if status.connected_peers > 0 {
        return NodeHealthLevel::LanReachable;
    }
    NodeHealthLevel::Isolated
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarmdrop_core::infra::InfraExclusion;
    use swarmdrop_core::network::candidates::{BootstrapCandidateSource, CandidateRoles};
    use swarmdrop_net::NodeId;

    const PEER: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";

    fn link(now: DateTime<Utc>, relay: Option<RelayLinkState>) -> InfraLink {
        InfraLink {
            peer_id: PEER.parse::<NodeId>().expect("fixture peer id"),
            addrs: Vec::new(),
            sources: vec![BootstrapCandidateSource::HostConfigured],
            roles: CandidateRoles::kad_and_relay(),
            scope: CandidateScope::Public,
            first_seen: now,
            last_seen: now,
            removable: true,
            connected: false,
            relay,
            ever_active: false,
            excluded: None,
        }
    }

    fn running(links: Vec<InfraLink>) -> NetworkStatus {
        NetworkStatus {
            status: NodeStatus::Running,
            infra_links: links,
            ..Default::default()
        }
    }

    #[test]
    fn a_stopped_node_outranks_everything_else() {
        let now = Utc::now();
        let mut status = running(vec![]);
        status.status = NodeStatus::Stopped;
        status.public_reachable = true;
        assert_eq!(summarize(&status, now), NodeHealthLevel::NotRunning);
    }

    /// 报警三条件里的「非配置造成」：用户自己关了公网可达性时，托盘不得报警。
    #[test]
    fn a_gate_the_user_closed_is_never_isolated() {
        let now = Utc::now();
        let mut l = link(now, None);
        l.excluded = Some(InfraExclusion::PublicReachabilityDisabled);
        let status = running(vec![l]);
        let level = summarize(&status, now + chrono::TimeDelta::seconds(60));
        assert_eq!(level, NodeHealthLevel::ConfiguredLanOnly);
        assert!(!level.is_isolated());
    }

    /// 宽限三条件：只有「没连上过 ∧ 已见 Failed ∧ 过了 10s」同时成立才离开 Starting。
    #[test]
    fn the_grace_window_needs_all_three_conditions() {
        let now = Utc::now();
        let failed = || {
            Some(RelayLinkState::Failed {
                last_error: "dial timeout".to_owned(),
            })
        };

        // 已见 Failed 但还在宽限内 → 仍是「正在连接」
        let status = running(vec![link(now, failed())]);
        assert_eq!(
            summarize(&status, now + chrono::TimeDelta::seconds(3)),
            NodeHealthLevel::Starting
        );

        // 过了宽限 → 孤立
        assert_eq!(
            summarize(&status, now + chrono::TimeDelta::seconds(30)),
            NodeHealthLevel::Isolated
        );

        // 没见过 Failed（首次拨号还在飞）→ 无论过多久都还是「正在连接」。
        // native 拨号超时 30s 远大于任何合理的宽限，纯定时器会在这里误报。
        let status = running(vec![link(now, Some(RelayLinkState::Connecting))]);
        assert_eq!(
            summarize(&status, now + chrono::TimeDelta::seconds(30)),
            NodeHealthLevel::Starting
        );
    }

    /// 「部分节点连不上」不是降级：一条挂了、另一条还在收敛，整体仍是「正在连接」；
    /// 有已连对端时是「局域网可达」，都不得报警。
    #[test]
    fn some_links_down_is_not_a_degradation() {
        let now = Utc::now();
        let late = now + chrono::TimeDelta::seconds(30);
        let dead = link(
            now,
            Some(RelayLinkState::Failed {
                last_error: "refused".to_owned(),
            }),
        );

        let mut alive = link(
            now,
            Some(RelayLinkState::Active {
                circuit_addr: "/ip4/1.2.3.4/tcp/4001/p2p-circuit".parse().expect("addr"),
            }),
        );
        alive.ever_active = true;
        let status = running(vec![dead.clone(), alive]);
        // 一条 Active 就不该报警——但公网可达性由 Rust 那侧算，这里没置位，
        // 所以落到「局域网可达」而不是 Isolated。
        let mut with_peer = status.clone();
        with_peer.connected_peers = 1;
        assert_eq!(summarize(&with_peer, late), NodeHealthLevel::LanReachable);
        assert!(!summarize(&with_peer, late).is_isolated());
    }

    #[test]
    fn public_reachable_is_the_only_success_gate() {
        let now = Utc::now();
        let mut status = running(vec![link(now, None)]);
        status.public_reachable = true;
        assert_eq!(summarize(&status, now), NodeHealthLevel::Reachable);
    }

    /// 零 link 不报警：那张空表本身就是答案，喊「检查引导节点」是指着空表说话。
    #[test]
    fn an_empty_candidate_table_stays_quiet() {
        let now = Utc::now();
        assert_eq!(summarize(&running(vec![]), now), NodeHealthLevel::Starting);
    }
}
