//! Parsing and construction of multiaddrs for both modes of this transport.
//!
//! # Hole punching (`/webrtc`)
//!
//! The spec has the dialed side append `/webrtc` to its own relayed multiaddr to
//! advertise support for this mode. The full on-the-wire shape (matching js-libp2p's
//! `private-to-private/transport.ts`):
//!
//! ```text
//! /ip4/1.2.3.4/tcp/4001/p2p/<relay>/p2p-circuit/webrtc/p2p/<target>
//!                                   ^^^^^^^^^^^^ ^^^^^^^ ^^^^^^^^^^
//!                                   circuit part  mode    target peer
//! ```
//!
//! **`/webrtc` sits between `p2p-circuit` and the target `/p2p/`**, not at the very end.
//! Put it in the wrong place and js-libp2p will no longer recognize the address, so this
//! module follows its `splitAddr` semantics and pins them with tests.
//!
//! # Direct (`/webrtc-direct`)
//!
//! ```text
//! /ip4/1.2.3.4/udp/4001/webrtc-direct/certhash/uEi…/p2p/<target>
//!               ^^^^^^^ ^^^^^^^^^^^^^ ^^^^^^^^^^^^^ ^^^^^^^^^^^^
//!               UDP     mode           cert digest   optional
//! ```
//!
//! # The two are never confused for each other
//!
//! `Protocol::WebRTC` and `Protocol::WebRTCDirect` are two distinct multiaddr protocol
//! segments, so telling them apart is unambiguous by construction. But a single
//! [`crate::Transport`] handles both, which makes the dispatch point **the one
//! architectural risk** — the tests assert in both directions (a `/webrtc` address is
//! never claimed by the direct path, and vice versa).

use std::net::{IpAddr, SocketAddr};

use libp2p_core::multiaddr::{Multiaddr, Protocol};
use libp2p_identity::PeerId;
use libp2p_webrtc_utils::Fingerprint;

/// Whether this is an address this transport can handle (contains a `/webrtc` segment).
pub fn is_webrtc(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::WebRTC)
}

/// Splits an address into "the address used to establish the signaling channel" and
/// "the target peer".
///
/// Semantics match js-libp2p's `splitAddr`:
/// - target = the **last** `/p2p/` segment (in a circuit address the relay occupies one
///   `/p2p/` of its own)
/// - signaling address = the original address **with the `/webrtc` segment removed**,
///   everything else left as-is
///
/// What remains after the removal is exactly a standard circuit address, ready to be
/// handed to the relay transport to dial.
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

/// Builds a dialable address for this transport from a circuit address and a target peer.
///
/// `circuit` should have the shape `…/p2p/<relay>/p2p-circuit`; the target `/p2p/<target>`
/// is appended by this function after `/webrtc`, so the circuit passed in **must not**
/// already contain the target segment.
pub fn from_circuit(circuit: &Multiaddr, target: PeerId) -> Multiaddr {
    let mut addr = circuit.clone();
    addr.push(Protocol::WebRTC);
    addr.push(Protocol::P2p(target));
    addr
}

/// Replaces the target peer in an address of this transport with `peer`.
///
/// Its purpose is to derive, from this side's **listen address**, the address the remote
/// would use to dial back through the same relay: a listen address has the shape
/// `…/p2p-circuit/webrtc/p2p/<self>`, and swapping the trailing `/p2p/<self>` yields the
/// address as seen from the remote.
///
/// If the last segment is not `/p2p`, it is simply appended — a listen address may omit
/// the local peer segment, and both shapes must produce the correct result.
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

// ── direct 模式（`/webrtc-direct`）──────────────────────────────────────────

/// Whether this is a direct-mode address (contains a `/webrtc-direct` segment).
///
/// Mutually exclusive with [`is_webrtc`]: the two are distinct multiaddr protocol
/// segments, so no single address matches both.
pub fn is_webrtc_direct(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::WebRTCDirect)
}

/// Parses a **dialable** direct-mode address, yielding the target socket and the
/// certificate fingerprint.
///
/// This reuses the `libp2p-webrtc-utils` implementation directly — **do not copy one in**:
/// the certhash encoding (multihash, SHA2-256 only) must be bit-for-bit identical to the
/// official one, or every existing address stops working. A trailing `/p2p/<id>` is
/// optional.
pub fn parse_direct_dial(addr: &Multiaddr) -> Option<(SocketAddr, Fingerprint)> {
    libp2p_webrtc_utils::parse_webrtc_dial_addr(addr)
}

/// Parses a **listen** address for direct mode.
///
/// How it differs from a dialable address: a listen address **carries no certhash** (the
/// local fingerprint is determined by the local certificate, so putting it in the address
/// is redundant — the official `parse_webrtc_listen_addr` rejects it too) and no `/p2p`
/// segment. Wildcard IPs and port 0 are allowed — `/ip4/0.0.0.0/udp/0/webrtc-direct` is
/// the most common form.
pub fn parse_direct_listen(addr: &Multiaddr) -> Option<SocketAddr> {
    let mut iter = addr.iter();

    let ip = match iter.next()? {
        Protocol::Ip4(ip) => IpAddr::from(ip),
        Protocol::Ip6(ip) => IpAddr::from(ip),
        _ => return None,
    };
    let Protocol::Udp(port) = iter.next()? else {
        return None;
    };
    let Protocol::WebRTCDirect = iter.next()? else {
        return None;
    };
    // 监听地址到此为止：多出任何一段都说明这不是给我们的。
    if iter.next().is_some() {
        return None;
    }

    Some(SocketAddr::new(ip, port))
}

/// Builds an advertisable direct-mode address from a socket address and the local
/// certificate fingerprint.
///
/// When `certhash` is `None` the result is the "source address as the remote sees it"
/// (`send_back_addr`) — that side's fingerprint is unknowable to us, and the official
/// implementation leaves it out as well.
pub fn direct_addr(socket: SocketAddr, certhash: Option<Fingerprint>) -> Multiaddr {
    let addr = Multiaddr::empty()
        .with(socket.ip().into())
        .with(Protocol::Udp(socket.port()))
        .with(Protocol::WebRTCDirect);

    match certhash {
        Some(fp) => addr.with(Protocol::Certhash(fp.to_multihash())),
        None => addr,
    }
}

/// Parses an SDP-form SHA-256 fingerprint (`aa:bb:…` or unseparated, case-insensitive).
///
/// Both sides need it but obtain it differently: native reads it from rtc's stats report,
/// the browser slices it out of the `a=fingerprint` line of `localDescription`. Obtaining
/// it is platform-specific; **decoding it is not**, so the decoding half lives here and is
/// shared.
///
/// SHA-256 (32 bytes) only — matching the certhash convention. Malformed input returns
/// `None` rather than panicking: this value comes from the browser on one side and from a
/// library's statistics API on the other, and neither should be assumed to always be
/// well-formed.
///
/// The [`Fingerprint`] in `libp2p-webrtc-utils` only offers one-way `to_sdp_format()`
/// output with no inverse, hence this implementation; the official `webrtc-websys` carries
/// a copy of its own (with an `unwrap`, no less).
pub fn parse_sdp_fingerprint(value: &str) -> Option<Fingerprint> {
    let nibbles: Vec<u8> = value
        .bytes()
        .filter(|b| *b != b':')
        .map(|b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;

    if nibbles.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in nibbles.chunks_exact(2).enumerate() {
        out[i] = (pair[0] << 4) | pair[1];
    }
    Some(Fingerprint::raw(out))
}

/// Address parsing error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a /webrtc address: {0}")]
    NotWebRtc(Multiaddr),
    #[error("address is missing the target peer's /p2p segment: {0}")]
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

        // webrtc-direct 是另一种模式，不能被打洞路径认走
        let direct: Multiaddr = "/ip4/1.2.3.4/udp/4001/webrtc-direct".parse().unwrap();
        assert!(!is_webrtc(&direct));
    }

    #[test]
    fn rejects_webrtc_addr_without_target() {
        let addr: Multiaddr = "/ip4/1.2.3.4/tcp/4001/p2p-circuit/webrtc".parse().unwrap();
        assert!(matches!(split(&addr), Err(Error::MissingTargetPeer(_))));
    }

    // ── direct 模式 ────────────────────────────────────────────────────────

    /// 官方 `parse_webrtc_dial_addr` 单测里的同一条地址与指纹。抄过来是**刻意**的：
    /// certhash 一旦与官方不一致，存量客户端通告的地址就全部拨不通。
    const OFFICIAL_ADDR: &str = "/ip4/127.0.0.1/udp/39901/webrtc-direct/certhash/uEiDikp5KVUgkLta1EjUN-IKbHk-dUBg8VzKgf5nXxLK46w";
    const OFFICIAL_DIGEST: [u8; 32] = [
        0xe2, 0x92, 0x9e, 0x4a, 0x55, 0x48, 0x24, 0x2e, 0xd6, 0xb5, 0x12, 0x35, 0x0d, 0xf8, 0x82,
        0x9b, 0x1e, 0x4f, 0x9d, 0x50, 0x18, 0x3c, 0x57, 0x32, 0xa0, 0x7f, 0x99, 0xd7, 0xc4, 0xb2,
        0xb8, 0xeb,
    ];

    #[test]
    fn parses_direct_dial_addr() {
        let addr: Multiaddr = OFFICIAL_ADDR.parse().unwrap();
        assert!(is_webrtc_direct(&addr));

        let (socket, fp) = parse_direct_dial(&addr).expect("应能解析");
        assert_eq!(socket, "127.0.0.1:39901".parse::<SocketAddr>().unwrap());
        assert_eq!(fp, Fingerprint::raw(OFFICIAL_DIGEST));

        // 末尾的 /p2p 段是可选的，带上也应解析成同一结果
        let with_p2p: Multiaddr = format!("{OFFICIAL_ADDR}/p2p/{}", peer(1)).parse().unwrap();
        assert_eq!(parse_direct_dial(&with_p2p), Some((socket, fp)));
    }

    /// 构造与解析必须互为逆运算——certhash 编码错了这条就红。
    #[test]
    fn direct_addr_roundtrips() {
        let socket: SocketAddr = "127.0.0.1:39901".parse().unwrap();
        let fp = Fingerprint::raw(OFFICIAL_DIGEST);

        let built = direct_addr(socket, Some(fp));
        assert_eq!(built.to_string(), OFFICIAL_ADDR, "必须与官方编码逐字一致");
        assert_eq!(parse_direct_dial(&built), Some((socket, fp)));

        // 无指纹形态（send_back_addr）不可拨，但仍应被认作 direct 地址
        let bare = direct_addr(socket, None);
        assert!(is_webrtc_direct(&bare));
        assert!(parse_direct_dial(&bare).is_none());
    }

    /// 监听地址不带 certhash，且允许通配 IP 与 0 端口。
    #[test]
    fn parses_direct_listen_addr() {
        for (s, expected) in [
            ("/ip4/0.0.0.0/udp/0/webrtc-direct", "0.0.0.0:0"),
            (
                "/ip4/192.168.1.5/udp/4003/webrtc-direct",
                "192.168.1.5:4003",
            ),
            ("/ip6/::1/udp/12345/webrtc-direct", "[::1]:12345"),
        ] {
            let addr: Multiaddr = s.parse().unwrap();
            assert_eq!(
                parse_direct_listen(&addr),
                Some(expected.parse::<SocketAddr>().unwrap()),
                "{s}"
            );
        }
    }

    /// 监听地址后面多出任何一段都不该被接受——带 certhash 的是**可拨**地址，
    /// 当成监听地址收下会绑到一个本机说了不算的指纹上。
    #[test]
    fn rejects_direct_listen_addr_with_trailing_segments() {
        for s in [
            OFFICIAL_ADDR,
            "/ip4/127.0.0.1/tcp/4003/webrtc-direct",
            "/ip4/127.0.0.1/udp/4003",
            "/ip4/127.0.0.1/udp/4003/quic-v1",
        ] {
            let addr: Multiaddr = s.parse().unwrap();
            assert!(parse_direct_listen(&addr).is_none(), "{s}");
        }
    }

    /// 分派点的反向断言：打洞地址绝不能被 direct 路径认领。
    ///
    /// 一个 [`crate::Transport`] 同时处理两种地址，这是唯一的架构风险点。
    #[test]
    fn direct_never_claims_hole_punch_addrs() {
        let (relay, target) = (peer(1), peer(2));
        let hole_punch = from_circuit(&circuit_of(relay), target);

        assert!(!is_webrtc_direct(&hole_punch));
        assert!(parse_direct_dial(&hole_punch).is_none());
        assert!(parse_direct_listen(&hole_punch).is_none());

        // 反向：direct 地址也不该被打洞路径认走
        let direct: Multiaddr = OFFICIAL_ADDR.parse().unwrap();
        assert!(!is_webrtc(&direct));
        assert!(matches!(split(&direct), Err(Error::NotWebRtc(_))));
    }

    /// 冒号分隔、连写、大小写混排都要认——浏览器给大写，rtc 的 stats 给小写。
    #[test]
    fn parses_sdp_fingerprint_in_every_shape() {
        let colon = "E2:92:9E:4A:55:48:24:2E:D6:B5:12:35:0D:F8:82:9B:\
1E:4F:9D:50:18:3C:57:32:A0:7F:99:D7:C4:B2:B8:EB";
        let expected = Fingerprint::raw(OFFICIAL_DIGEST);

        assert_eq!(parse_sdp_fingerprint(colon), Some(expected));
        assert_eq!(parse_sdp_fingerprint(&colon.to_lowercase()), Some(expected));
        assert_eq!(
            parse_sdp_fingerprint(&colon.replace(':', "")),
            Some(expected)
        );
    }

    /// 畸形输入返回 None 而不是给出半截指纹——错误的指纹会让 Noise 的 prologue
    /// 对不上，症状是「握手莫名失败」，比直接报错难查得多。
    #[test]
    fn rejects_malformed_sdp_fingerprint() {
        for bad in [
            "",
            "aa:bb",
            &"zz".repeat(32),
            &"ab".repeat(31),
            &"ab".repeat(33),
        ] {
            assert!(parse_sdp_fingerprint(bad).is_none(), "{bad}");
        }
    }

    /// 只支持 SHA2-256。换了哈希函数就解析不出——官方同样如此，
    /// 放行会让两端算出不同的指纹而 DTLS 校验失败。
    #[test]
    fn rejects_non_sha256_certhash() {
        let addr: Multiaddr =
            "/ip6/::1/udp/12345/webrtc-direct/certhash/uFiCH_tkkzpAwkoIDbE4I7QtQksFMYs5nQ4MyYrkgCJYi4A"
                .parse()
                .unwrap();
        assert!(is_webrtc_direct(&addr), "协议段仍在，只是哈希函数不对");
        assert!(parse_direct_dial(&addr).is_none());
    }
}
