//! 本传输两种模式的 multiaddr 解析与构造。
//!
//! # 打洞（`/webrtc`）
//!
//! spec 规定被拨方在自己的 relayed multiaddr 后追加 `/webrtc` 来通告支持本模式。
//! 线上的完整形态（与 js-libp2p `private-to-private/transport.ts` 对齐）：
//!
//! ```text
//! /ip4/1.2.3.4/tcp/4001/p2p/<relay>/p2p-circuit/webrtc/p2p/<target>
//!                                   ^^^^^^^^^^^^ ^^^^^^^ ^^^^^^^^^^
//!                                   circuit 段   本模式   目标节点
//! ```
//!
//! **`/webrtc` 夹在 `p2p-circuit` 与目标 `/p2p/` 之间**，不是缀在最末尾。位置错了
//! 就与 js-libp2p 互不相认，故这里按它的 `splitAddr` 语义实现并用测试钉死。
//!
//! # direct（`/webrtc-direct`）
//!
//! ```text
//! /ip4/1.2.3.4/udp/4001/webrtc-direct/certhash/uEi…/p2p/<target>
//!               ^^^^^^^ ^^^^^^^^^^^^^ ^^^^^^^^^^^^^ ^^^^^^^^^^^^
//!               UDP     本模式         证书指纹      可选
//! ```
//!
//! # 两者互不误认
//!
//! `Protocol::WebRTC` 与 `Protocol::WebRTCDirect` 是 multiaddr 的两个不同协议段，
//! 判别天然不冲突。但一个 [`crate::Transport`] 同时处理两者，分派点是**唯一的架构风险**
//! ——测试对两个方向都做了反向断言（`/webrtc` 不被 direct 认领，反之亦然）。

use std::net::{IpAddr, SocketAddr};

use libp2p_core::multiaddr::{Multiaddr, Protocol};
use libp2p_identity::PeerId;
use libp2p_webrtc_utils::Fingerprint;

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

// ── direct 模式（`/webrtc-direct`）──────────────────────────────────────────

/// 判断是否为 direct 模式的地址（含 `/webrtc-direct` 段）。
///
/// 与 [`is_webrtc`] 互斥：两者是 multiaddr 的不同协议段，同一地址不会同时命中。
pub fn is_webrtc_direct(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::WebRTCDirect)
}

/// 解析 direct 模式的**可拨**地址，取出目标 socket 与证书指纹。
///
/// 直接复用 `libp2p-webrtc-utils` 的实现，**不要自己抄一份**：certhash 的编码
/// （multihash + 只认 SHA2-256）必须与官方逐位一致，否则存量地址全部失效。
/// 末尾的 `/p2p/<id>` 可有可无。
pub fn parse_direct_dial(addr: &Multiaddr) -> Option<(SocketAddr, Fingerprint)> {
    libp2p_webrtc_utils::parse_webrtc_dial_addr(addr)
}

/// 解析 direct 模式的**监听**地址。
///
/// 与可拨地址的区别：监听地址**不带 certhash**（本机指纹由本机证书决定，写在地址里
/// 是多余的，官方 `parse_webrtc_listen_addr` 同样拒绝），也不带 `/p2p` 段。
/// 允许通配 IP 与 0 端口——`/ip4/0.0.0.0/udp/0/webrtc-direct` 是最常见的写法。
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

/// 由 socket 地址与本机证书指纹构造 direct 模式的可通告地址。
///
/// `certhash` 为 `None` 时产出的是「对端视角的来源地址」（`send_back_addr`）——
/// 那一侧的指纹我们无从得知，官方同样留空。
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

/// 解析 SDP 形态的 SHA-256 指纹（`aa:bb:…` 或连写，大小写不敏感）。
///
/// 两侧都要它、但取值来源不同：native 从 rtc 的 stats 报告里读，浏览器从
/// `localDescription` 的 `a=fingerprint` 行里切。取值那半是平台相关的，**解码这半不是**，
/// 所以放在这里共用。
///
/// 只认 SHA-256（32 字节）——与 certhash 的约定一致。畸形输入返回 `None` 而不是 panic：
/// 这条数据一侧来自浏览器、一侧来自库的统计接口，都不该假定它永远规整。
///
/// `libp2p-webrtc-utils` 的 [`Fingerprint`] 只有 `to_sdp_format()` 单向输出，没有反解，
/// 所以自己写；官方 `webrtc-websys` 里也抄了一份（还带 `unwrap`）。
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
