//! 引导节点地址的**提交前同步校验**。
//!
//! # 为什么不做独立的连通性探测
//!
//! `Endpoint::connect` 有三个副作用使它不适合当探测原语：它把候选地址**永久**写进
//! address_book（无 TTL、无失败回滚，清理入口是会断连的 `remove_infrastructure_peer`）；
//! 已连接时直接返回既有连接快照，所以对内置节点**永远绿**——一个不可能失败的测试比
//! 没有测试更坏；而且它走直连，relay 的实际用法是 reservation，两条链路不同。
//!
//! 取而代之是两段：**本模块**（零网络成本、100% 确定的形状校验）+ 提交后由收敛环给
//! 答案（`ensure_infra_intent` 同步返回，supervisor 最迟 1s 起第一轮，状态经
//! `watch_relays` 落定）。后者测的就是后续收敛真正会走的那条链路。
//!
//! # 为什么住在 core 而不是三端各写一份
//!
//! 三条规则里有两条要认识内核事实：合法的 peer id 形状、以及**本端点实际装配了哪些
//! transport**。后者是 Web 端最容易踩的（往浏览器里粘一条 `/tcp/`），而三端各写一遍
//! 必然漂移——桌面写完忘了浏览器没有 TCP，浏览器写完忘了 native 没有 `/webtransport`。

use std::collections::BTreeSet;

use swarmdrop_net::{Addr, Endpoint, NodeAddr, NodeId, TransportKind};

use crate::network::candidates::BootstrapCandidate;

/// 提交前校验失败的原因。
///
/// 每个变体都对应一句**用户能据以行动**的话。刻意不设 `Other(String)` 兜底：
/// 那会让新增的失败原因静默退化成一句无从下手的原始错误。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InfraAddrError {
    /// 不是合法的 multiaddr。
    #[serde(rename_all = "camelCase")]
    Malformed { detail: String },
    /// 缺少 `/p2p/<peer-id>` 段——没有身份就无法验证连上的是不是同一台。
    MissingPeerId,
    /// 地址里不含任何可拨传输段（如只有 `/p2p/<id>`）。
    NoTransport,
    /// 传输本端点没有装配。
    #[serde(rename_all = "camelCase")]
    UnsupportedTransport {
        /// 地址里那个传输的 wire 名（`tcp` / `quic` / `webrtc` / `webrtcDirect` /
        /// `webtransport`）。
        transport: String,
        /// 本端点支持的传输，供 UI 直接列出来。
        supported: Vec<String>,
    },
    /// 指向本机。
    SelfAddr,
    /// 与既有条目重复（含内置清单）。
    Duplicate,
}

/// 校验通过的地址。
///
/// 刻意不直接返回 `NodeAddr`：调用方需要知道「这条本来就在表里」与「这是新的」的
/// 区别——前者不该报错（幂等 upsert 会合并地址），但 UI 要给的反馈不同。
pub type InfraAddrResult = Result<NodeAddr, InfraAddrError>;

/// 校验一条用户输入的引导节点地址。
///
/// 五条规则全部**无网络往返**：可解析 → 含合法 peer id → 含可拨传输 → 该传输本端装配了
/// → 不是本机、不与既有条目重复。
///
/// 去重按 `peer_id` 而非整串地址：同一个节点的 TCP 与 QUIC 地址是同一条关系的两条路径，
/// 幂等 upsert 会把它们合并，报「重复」反而是错的。所以 `existing` 里已有该 peer 时
/// **不算重复**——只有连地址集都一模一样才是真的什么都没变。
pub fn validate_infra_addr(
    input: &str,
    endpoint: &Endpoint,
    existing: &[BootstrapCandidate],
) -> InfraAddrResult {
    validate_against(
        input,
        endpoint.node_id(),
        endpoint.supported_transports(),
        existing,
    )
}

/// [`validate_infra_addr`] 的纯判据部分。
///
/// 与门面分开，是因为造一个 [`Endpoint`] 要真的 bind 一次——那会让这五条规则唯一的
/// 单测变成一个起网络的集成测试，而它们全都不需要网络。
pub fn validate_against(
    input: &str,
    self_id: NodeId,
    supported: &[TransportKind],
    existing: &[BootstrapCandidate],
) -> InfraAddrResult {
    let addr: Addr = input
        .trim()
        .parse()
        .map_err(
            |e: swarmdrop_net::AddrParseError| InfraAddrError::Malformed {
                detail: e.to_string(),
            },
        )?;

    let peer_id = addr.p2p_node_id().ok_or(InfraAddrError::MissingPeerId)?;
    if peer_id == self_id {
        return Err(InfraAddrError::SelfAddr);
    }

    let transport = addr.transport().ok_or(InfraAddrError::NoTransport)?;
    if !supported.contains(&transport) {
        return Err(InfraAddrError::UnsupportedTransport {
            transport: TransportKind::wire_name(transport).to_owned(),
            supported: supported
                .iter()
                .map(|k| TransportKind::wire_name(*k).to_owned())
                .collect(),
        });
    }

    if is_exact_duplicate(peer_id, &addr, existing) {
        return Err(InfraAddrError::Duplicate);
    }

    Ok(NodeAddr {
        id: peer_id,
        addrs: vec![addr],
    })
}

/// 「这条地址已经在表里了」——同一 peer **且**该地址已登记。
///
/// 同 peer 的新地址不算重复：那是给一条既有关系补一条路径，upsert 会合并。
fn is_exact_duplicate(peer_id: NodeId, addr: &Addr, existing: &[BootstrapCandidate]) -> bool {
    existing
        .iter()
        .filter(|c| c.peer_id == peer_id)
        .any(|c| c.addrs.iter().any(|a| a == addr))
}

/// 本端点装配的传输的 wire 名清单（UI 拿去写「本端支持 …」）。
pub fn supported_transport_names(endpoint: &Endpoint) -> BTreeSet<String> {
    endpoint
        .supported_transports()
        .iter()
        .map(|k| TransportKind::wire_name(*k).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};
    use chrono::Utc;

    const RELAY: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
    const SELF: &str = "12D3KooWRBhwfeP2Y4TCx1SM6s9rUoHhR5STiGwxBhgFRcw3UERE";
    const TARGET: &str = "12D3KooWH3uVF6wv47WnArKHk5p6cvgCJEb74UTmxztmQDc298L3";

    fn native() -> Vec<TransportKind> {
        vec![
            TransportKind::Tcp,
            TransportKind::Quic,
            TransportKind::WebrtcDirect,
        ]
    }

    fn browser() -> Vec<TransportKind> {
        vec![TransportKind::WebrtcDirect, TransportKind::Webrtc]
    }

    fn self_id() -> NodeId {
        SELF.parse().expect("fixture peer id")
    }

    fn check(input: &str, supported: &[TransportKind]) -> InfraAddrResult {
        validate_against(input, self_id(), supported, &[])
    }

    fn candidate(addr: &str) -> BootstrapCandidate {
        let addr: Addr = addr.parse().expect("fixture addr");
        BootstrapCandidate {
            peer_id: addr.p2p_node_id().expect("fixture has /p2p/"),
            addrs: vec![addr],
            sources: vec![BootstrapCandidateSource::HostConfigured],
            roles: CandidateRoles::kad_and_relay(),
            scope: CandidateScope::Public,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        }
    }

    #[test]
    fn accepts_a_well_formed_address_for_an_assembled_transport() {
        let ok = check(
            &format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}"),
            &native(),
        )
        .expect("native 装配了 tcp");
        assert_eq!(ok.id.to_string(), RELAY);
        assert_eq!(ok.addrs.len(), 1);

        // 前后空白是粘贴的常态，不该成为一条错误
        check(
            &format!("  /ip4/47.115.172.218/udp/4001/quic-v1/p2p/{RELAY}\n"),
            &native(),
        )
        .expect("空白应被裁掉");
    }

    /// 这是本函数存在的主要理由：往浏览器里粘一条 `/tcp/`，反过来往桌面粘一条只有
    /// 浏览器有的传输。两边今天都会静静地收下，然后永远连不上且没有任何提示。
    #[test]
    fn rejects_transports_the_local_endpoint_never_assembled() {
        let err = check(
            &format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}"),
            &browser(),
        )
        .expect_err("浏览器没有 tcp");
        let InfraAddrError::UnsupportedTransport {
            transport,
            supported,
        } = err
        else {
            panic!("应当报 UnsupportedTransport，实际 {err:?}");
        };
        assert_eq!(transport, "tcp");
        // 错误里带着本端支持什么，UI 才说得出下一步
        assert!(supported.contains(&"webrtcDirect".to_owned()));
        assert!(!supported.contains(&"tcp".to_owned()));

        assert!(matches!(
            check(
                &format!("/ip4/1.2.3.4/udp/4001/webrtc/p2p/{RELAY}"),
                &native()
            ),
            Err(InfraAddrError::UnsupportedTransport { .. })
        ));
    }

    #[test]
    fn rejects_addresses_without_an_identity_or_a_transport() {
        assert!(matches!(
            check("/ip4/47.115.172.218/tcp/4001", &native()),
            Err(InfraAddrError::MissingPeerId)
        ));
        assert!(matches!(
            check(&format!("/p2p/{RELAY}"), &native()),
            Err(InfraAddrError::NoTransport)
        ));
        assert!(matches!(
            check("not a multiaddr", &native()),
            Err(InfraAddrError::Malformed { .. })
        ));
    }

    #[test]
    fn rejects_our_own_node() {
        assert!(matches!(
            check(
                &format!("/ip4/47.115.172.218/tcp/4001/p2p/{SELF}"),
                &native()
            ),
            Err(InfraAddrError::SelfAddr)
        ));
    }

    /// 去重按「同 peer **且**同地址」。同一节点的另一条路径不是重复——upsert 会合并，
    /// 报重复反而挡住了用户补一条 QUIC 地址。
    #[test]
    fn duplicate_means_the_same_address_not_merely_the_same_peer() {
        let tcp = format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}");
        let quic = format!("/ip4/47.115.172.218/udp/4001/quic-v1/p2p/{RELAY}");
        let existing = vec![candidate(&tcp)];

        assert!(matches!(
            validate_against(&tcp, self_id(), &native(), &existing),
            Err(InfraAddrError::Duplicate)
        ));
        validate_against(&quic, self_id(), &native(), &existing)
            .expect("同一节点的另一条路径应当放行");
    }

    /// circuit 地址的传输取的是**外层中继段**——那正是本机要拨的那一跳，
    /// 而不是它背后那个目标节点用什么连上来的。
    #[test]
    fn circuit_addresses_are_judged_by_the_hop_we_actually_dial() {
        let circuit = format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}");

        // 浏览器拨不动那一跳的 tcp
        assert!(matches!(
            validate_against(&circuit, self_id(), &browser(), &[]),
            Err(InfraAddrError::UnsupportedTransport { transport, .. }) if transport == "tcp"
        ));

        // native 可以，且身份取的是末位（目标）而非中继
        let ok = validate_against(&circuit, self_id(), &native(), &[]).expect("native 有 tcp");
        assert_eq!(ok.id.to_string(), TARGET);
    }
}
