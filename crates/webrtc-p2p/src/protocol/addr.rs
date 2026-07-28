//! `/webrtc` multiaddr 的解析与构造。
//!
//! spec 规定被拨方在自己的 relayed multiaddr 后追加 `/webrtc` 来通告支持本传输。
//! 线上的完整形态（与 js-libp2p `private-to-private/transport.ts` 对齐）：
//!
//! ```text
//! /ip4/1.2.3.4/tcp/4001/p2p/<relay>/p2p-circuit/webrtc/p2p/<target>
//!                                   ^^^^^^^^^^^^ ^^^^^^^ ^^^^^^^^^^
//!                                   circuit 段   本传输   目标节点
//! ```
//!
//! **`/webrtc` 夹在 `p2p-circuit` 与目标 `/p2p/` 之间**，不是缀在最末尾。位置错了
//! 就与 js-libp2p 互不相认，故这里按它的 `splitAddr` 语义实现并用测试钉死。

use libp2p_core::multiaddr::{Multiaddr, Protocol};
use libp2p_identity::PeerId;

/// 判断是否为本传输能处理的地址（含 `/webrtc` 段）。
pub fn is_webrtc(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::WebRTC)
}

/// 拆出「用于建立信令通道的地址」与「目标节点」。
///
/// 语义对齐 js-libp2p 的 `splitAddr`：
/// - 目标 = **最后一个** `/p2p/` 段（circuit 地址里 relay 自己也占一个 `/p2p/`）
/// - 信令地址 = 原地址**剔除 `/webrtc` 段**，其余原样保留
///
/// 剔除后得到的正是一个标准 circuit 地址，可直接交给 relay transport 拨号。
pub fn split(addr: &Multiaddr) -> Result<(Multiaddr, PeerId), Error> {
    if !is_webrtc(addr) {
        return Err(Error::NotWebRtc(addr.clone()));
    }

    let target = addr
        .iter()
        .filter_map(|p| match p {
            Protocol::P2p(id) => Some(id),
            _ => None,
        })
        .last()
        .ok_or_else(|| Error::MissingTargetPeer(addr.clone()))?;

    let signaling = addr
        .iter()
        .filter(|p| *p != Protocol::WebRTC)
        .collect::<Multiaddr>();

    Ok((signaling, target))
}

/// 由 circuit 地址与目标节点构造本传输的可拨地址。
///
/// `circuit` 应是 `…/p2p/<relay>/p2p-circuit` 形态；目标 `/p2p/<target>` 由本函数补在
/// `/webrtc` 之后，故传入的 circuit **不应**已含目标段。
pub fn from_circuit(circuit: &Multiaddr, target: PeerId) -> Multiaddr {
    let mut addr = circuit.clone();
    addr.push(Protocol::WebRTC);
    addr.push(Protocol::P2p(target));
    addr
}

/// 把本传输地址里的目标节点换成 `peer`。
///
/// 用途是由本端的**监听地址**推出「对端经同一 relay 回拨过来」的地址：监听地址形如
/// `…/p2p-circuit/webrtc/p2p/<self>`，换掉末位的 `/p2p/<self>` 即得对端视角的地址。
///
/// 末位不是 `/p2p` 段时直接追加——监听地址允许省略本机段，两种形态都能得到正确结果。
pub fn with_peer(addr: &Multiaddr, peer: PeerId) -> Multiaddr {
    let mut parts: Vec<Protocol> = addr.iter().collect();
    if matches!(parts.last(), Some(Protocol::P2p(_))) {
        parts.pop();
    }
    parts
        .into_iter()
        .collect::<Multiaddr>()
        .with(Protocol::P2p(peer))
}

/// 地址解析错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("不是 /webrtc 地址：{0}")]
    NotWebRtc(Multiaddr),
    #[error("地址缺少目标节点的 /p2p 段：{0}")]
    MissingTargetPeer(Multiaddr),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(seed: u8) -> PeerId {
        PeerId::from_bytes(
            &libp2p_identity::Keypair::ed25519_from_bytes([seed; 32])
                .unwrap()
                .public()
                .to_peer_id()
                .to_bytes(),
        )
        .unwrap()
    }

    fn circuit_of(relay: PeerId) -> Multiaddr {
        format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit")
            .parse()
            .unwrap()
    }

    #[test]
    fn splits_relay_and_target() {
        let (relay, target) = (peer(1), peer(2));
        let addr = from_circuit(&circuit_of(relay), target);

        // 线上形态：/webrtc 夹在 p2p-circuit 与目标 /p2p 之间
        assert_eq!(
            addr.to_string(),
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/webrtc/p2p/{target}")
        );

        let (signaling, got) = split(&addr).unwrap();
        assert_eq!(got, target, "目标应取最后一个 /p2p 段，而非 relay 的那个");
        assert_eq!(
            signaling.to_string(),
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/p2p/{target}"),
            "剔除 /webrtc 后应是可直接拨号的标准 circuit 地址"
        );
        assert!(!is_webrtc(&signaling));
    }

    /// relay 自己也占一个 /p2p 段——取「第一个」就会把 relay 当成目标。
    #[test]
    fn target_is_last_p2p_not_relay() {
        let (relay, target) = (peer(3), peer(4));
        let (_, got) = split(&from_circuit(&circuit_of(relay), target)).unwrap();
        assert_eq!(got, target);
        assert_ne!(got, relay);
    }

    /// 监听地址 → 对端回拨地址：只换目标段，relay 与 `/webrtc` 位置不动。
    #[test]
    fn with_peer_replaces_target() {
        let (relay, me, remote) = (peer(5), peer(6), peer(7));
        let listen = from_circuit(&circuit_of(relay), me);

        let back = with_peer(&listen, remote);
        assert_eq!(
            back.to_string(),
            format!("/ip4/1.2.3.4/tcp/4001/p2p/{relay}/p2p-circuit/webrtc/p2p/{remote}")
        );
        assert_eq!(split(&back).unwrap().1, remote);
    }

    /// 监听地址省略本机 `/p2p` 段时应当是追加而非替换——否则会把 relay 段吃掉。
    #[test]
    fn with_peer_appends_when_no_target() {
        let relay = peer(8);
        let back = with_peer(&from_circuit_base(relay), peer(9));
        assert_eq!(split(&back).unwrap().1, peer(9));
        assert!(
            back.iter().any(|p| p == Protocol::P2p(relay)),
            "relay 段必须原样保留"
        );
    }

    fn from_circuit_base(relay: PeerId) -> Multiaddr {
        circuit_of(relay).with(Protocol::WebRTC)
    }

    #[test]
    fn rejects_non_webrtc_addr() {
        let plain: Multiaddr = "/ip4/1.2.3.4/tcp/4001".parse().unwrap();
        assert!(!is_webrtc(&plain));
        assert!(matches!(split(&plain), Err(Error::NotWebRtc(_))));

        // webrtc-direct 是另一个传输，不能被误认
        let direct: Multiaddr = "/ip4/1.2.3.4/udp/4001/webrtc-direct".parse().unwrap();
        assert!(!is_webrtc(&direct));
    }

    #[test]
    fn rejects_webrtc_addr_without_target() {
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001/p2p-circuit/webrtc".parse().unwrap();
        assert!(matches!(split(&addr), Err(Error::MissingTargetPeer(_))));
    }
}
