//! multiaddr ⇄ `(SocketAddr, certhash)` 的纯函数层。
//!
//! **零 IO、零 `wtransport`、零 `libp2p-swarm`** —— 整个模块可单测，也是全 crate 唯一
//! 知道 multiaddr 长什么样的地方。
//!
//! # 两种地址，两套判据
//!
//! | | 监听地址 | 通告 / 可拨地址 |
//! |---|---|---|
//! | 形态 | `/ip4/0.0.0.0/udp/4004/quic-v1/webtransport` | `…/webtransport/certhash/<h1>/certhash/<h2>[/p2p/<id>]` |
//! | certhash | **必须没有** | 一到多个 |
//! | 通配 IP / 端口 0 | 允许 | 无意义（通告前已展开成具体网卡地址） |
//!
//! 监听地址不带 certhash 不是可选的宽松：本机指纹由本机证书决定，写进监听地址只会
//! 制造一个可以与实际证书矛盾的第二事实源。官方 `parse_webrtc_listen_addr` 同样拒绝它。
//!
//! # `/quic` 与 `/quic-v1`
//!
//! 解析时**两者都接受**（与上游 `libp2p-webtransport-websys` 一致），通告时一律用
//! `/quic-v1` —— go-libp2p 与 js-libp2p 的既有部署都是它，spec 文档里的 `/quic` 是
//! draft 时期的老写法。

use std::net::{IpAddr, SocketAddr};

use libp2p_core::multiaddr::{Multiaddr, Protocol};
use libp2p_core::multihash::Multihash;
use libp2p_identity::PeerId;

/// SHA2-256 在 multihash 表里的编码。
///
/// libp2p 的 certhash **只允许**这一种摘要，因此没有「按 code 分派」的必要，
/// 反过来收到别的 code 必须拒绝而不是尽力而为。
const SHA2_256: u64 = 0x12;

/// 一张证书的 SHA-256 摘要 —— 本 crate 内 certhash 的唯一表示。
///
/// 同一个摘要在三处以三种编码出现：multiaddr 的 `/certhash/` 段（multihash）、
/// Noise 的 `webtransport_certhashes` 扩展（`Multihash<64>`）、TLS 校验（32 字节裸摘要）。
/// 收口成一个类型后，编码转换只在本文件发生一次。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Certhash([u8; 32]);

impl Certhash {
    /// 由 32 字节的 SHA-256 摘要构造。
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// 裸摘要。给 TLS 层做证书校验用。
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// multihash 编码 —— multiaddr 段与 Noise 扩展共用这一种。
    ///
    /// # Panics
    ///
    /// 不会。`Multihash::<64>::wrap` 只在摘要长度超过 64 时失败，而这里恒为 32。
    pub fn to_multihash(self) -> Multihash<64> {
        Multihash::wrap(SHA2_256, &self.0).expect("32 字节摘要必然能装进 Multihash<64>")
    }

    /// 从 multihash 还原。
    ///
    /// code 不是 SHA2-256 或长度不是 32 时返回 `None` —— 这两种输入都只可能来自
    /// 伪造或另一套实现，静默接受会让 certhash 校验形同虚设。
    pub fn from_multihash(hash: &Multihash<64>) -> Option<Self> {
        if hash.code() != SHA2_256 {
            return None;
        }
        hash.digest().try_into().ok().map(Self)
    }
}

impl std::fmt::Debug for Certhash {
    /// 打印成 multiaddr 里那一段的原文（`uEi…`），排障时可直接与日志/地址对照。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Protocol::Certhash(self.to_multihash()))
    }
}

/// 一条可拨的 WebTransport 地址拆出来的三件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialAddr {
    /// 目标 UDP socket。
    pub socket: SocketAddr,
    /// 本端「愿意接受」的证书摘要，**保序**。
    ///
    /// 顺序对拨号本身无影响（是个集合语义），但保序让日志与通告地址逐字对得上。
    pub certhashes: Vec<Certhash>,
    /// 地址末尾声明的对端身份，可缺省。
    pub peer: Option<PeerId>,
}

/// 是否是本 transport 认领的地址（含 `/webtransport` 段）。
pub fn is_webtransport(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::WebTransport)
}

/// 解析一条**可拨**地址。
///
/// 不认领时返回 `None`（交给地址分派链上的下一个 transport），认领但形态不合法时也返回
/// `None` —— 对 `libp2p_core::Transport::dial` 而言两者都是「这条地址不归我」，区分它们
/// 只会让调用方多一个无从处理的错误分支。
pub fn parse_dial(addr: &Multiaddr) -> Option<DialAddr> {
    let mut ip = None;
    let mut port = None;
    let mut quic = false;
    let mut webtransport = false;
    let mut certhashes = Vec::new();
    let mut peer = None;

    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(v4) => ip = Some(IpAddr::V4(v4)),
            Protocol::Ip6(v6) => ip = Some(IpAddr::V6(v6)),
            Protocol::Udp(p) => port = Some(p),
            // 两种写法都收，通告时统一用 QuicV1（见模块文档）。
            Protocol::Quic | Protocol::QuicV1 => quic = true,
            Protocol::WebTransport => webtransport = true,
            Protocol::Certhash(hash) => certhashes.push(Certhash::from_multihash(&hash)?),
            Protocol::P2p(id) => peer = Some(id),
            // 其余段（含 `/p2p-circuit`）一律不认：WebTransport 是直连传输，
            // 出现中继段说明这条地址属于 relay transport。
            _ => return None,
        }
    }

    if !quic || !webtransport {
        return None;
    }

    Some(DialAddr {
        socket: SocketAddr::new(ip?, port?),
        certhashes,
        peer,
    })
}

/// 解析一条**监听**地址。
///
/// 与 [`parse_dial`] 的差别只有一条：监听地址**不得带 certhash 或 `/p2p`**。
/// 通配 IP 与端口 0 则是允许的（`/ip4/0.0.0.0/udp/0/quic-v1/webtransport` 是最常见的写法）。
///
/// 实现直接复用 `parse_dial` 的解析，只加那一条额外判据 —— 两份各自遍历一遍的话，
/// 将来放行任何新段（`/dns4`、`/ip6zone`）都要改两处，而漏掉一处的表现是
/// 「拨得通但监听不了」或反之，**两个循环都编得过、都不报错**。
pub fn parse_listen(addr: &Multiaddr) -> Option<SocketAddr> {
    let parsed = parse_dial(addr)?;

    (parsed.certhashes.is_empty() && parsed.peer.is_none()).then_some(parsed.socket)
}

/// 拼一条可通告的地址。
pub fn advertise_addr(socket: SocketAddr, certhashes: &[Certhash]) -> Multiaddr {
    let mut addr = Multiaddr::empty()
        .with(match socket.ip() {
            IpAddr::V4(v4) => Protocol::Ip4(v4),
            IpAddr::V6(v6) => Protocol::Ip6(v6),
        })
        .with(Protocol::Udp(socket.port()))
        .with(Protocol::QuicV1)
        .with(Protocol::WebTransport);

    for hash in certhashes {
        addr.push(Protocol::Certhash(hash.to_multihash()));
    }
    addr
}

/// 由实际绑定的 socket 推出要通告的地址清单。
///
/// 绑在通配地址（`0.0.0.0` / `::`）上时**必须展开成具体网卡地址** —— 通配地址通告出去
/// 对端没法拨。枚举网卡失败时原样通告并留 warn，至少不是静默什么都不报。
pub fn announce_addrs(bound: SocketAddr, certhashes: &[Certhash]) -> Vec<Multiaddr> {
    if !bound.ip().is_unspecified() {
        return vec![advertise_addr(bound, certhashes)];
    }

    let want_v4 = bound.is_ipv4();
    let addrs: Vec<Multiaddr> = if_addrs::get_if_addrs()
        .map(|ifaces| {
            ifaces
                .into_iter()
                .map(|i| i.addr.ip())
                .filter(|ip| ip.is_ipv4() == want_v4)
                // 回环通告出去只对同机有意义，而那正是集成测试与「本机浏览器连本机桌面」
                // 两个真实场景，留着。
                .map(|ip| advertise_addr(SocketAddr::new(ip, bound.port()), certhashes))
                .collect()
        })
        .unwrap_or_default();

    if addrs.is_empty() {
        tracing::warn!(%bound, "未能枚举网卡，webtransport 直接通告通配地址（对端多半拨不通）");
        return vec![advertise_addr(bound, certhashes)];
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(seed: u8) -> Certhash {
        Certhash::new([seed; 32])
    }

    fn dial_addr(certhashes: &[Certhash]) -> Multiaddr {
        advertise_addr("1.2.3.4:4004".parse().unwrap(), certhashes)
    }

    /// certhash 列表必须**保序**取出：通告地址里 current 在前、next 在后，
    /// 顺序丢了日志就与地址对不上。
    #[test]
    fn parses_dial_addr_preserving_certhash_order() {
        let parsed = parse_dial(&dial_addr(&[hash(1), hash(2)])).expect("应认领");

        assert_eq!(parsed.socket, "1.2.3.4:4004".parse().unwrap());
        assert_eq!(parsed.certhashes, vec![hash(1), hash(2)]);
        assert_eq!(parsed.peer, None);
    }

    #[test]
    fn parses_trailing_p2p_segment() {
        let peer = PeerId::random();
        let addr = dial_addr(&[hash(1)]).with(Protocol::P2p(peer));

        assert_eq!(parse_dial(&addr).unwrap().peer, Some(peer));
    }

    /// 老写法 `/quic` 也要认 —— spec 文档里就是它，存量地址可能带着。
    #[test]
    fn accepts_legacy_quic_segment() {
        let addr: Multiaddr = "/ip4/1.2.3.4/udp/4004/quic/webtransport".parse().unwrap();
        assert_eq!(
            parse_listen(&addr),
            Some("1.2.3.4:4004".parse::<SocketAddr>().unwrap())
        );
    }

    /// 通告一律用 `/quic-v1`：go-libp2p / js-libp2p 的既有部署都是它。
    #[test]
    fn advertises_quic_v1() {
        let addr = advertise_addr("1.2.3.4:4004".parse().unwrap(), &[hash(1)]);
        assert!(addr.iter().any(|p| p == Protocol::QuicV1));
        assert!(!addr.iter().any(|p| p == Protocol::Quic));
    }

    /// 监听地址带 certhash 必须拒 —— 本机指纹由本机证书决定，
    /// 地址里再写一份就是可以与实际矛盾的第二事实源。
    #[test]
    fn rejects_listen_addr_carrying_certhash() {
        assert_eq!(parse_listen(&dial_addr(&[hash(1)])), None);
    }

    /// 通配 IP 与端口 0 是最常见的监听写法，必须接受。
    #[test]
    fn accepts_wildcard_listen_addr() {
        let addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1/webtransport".parse().unwrap();
        assert_eq!(
            parse_listen(&addr),
            Some("0.0.0.0:0".parse::<SocketAddr>().unwrap())
        );
    }

    /// 没有 `/webtransport` 段的 QUIC 地址不归本 transport —— 必须让给 libp2p-quic，
    /// 而不是报错。
    #[test]
    fn does_not_claim_plain_quic() {
        let addr: Multiaddr = "/ip4/1.2.3.4/udp/4001/quic-v1".parse().unwrap();

        assert!(!is_webtransport(&addr));
        assert_eq!(parse_dial(&addr), None);
        assert_eq!(parse_listen(&addr), None);
    }

    /// circuit 地址属于 relay transport。含 `/p2p-circuit` 却带 `/webtransport` 的地址
    /// 不能被认领 —— 认领了就会拿一条中继地址去直连。
    #[test]
    fn does_not_claim_circuit_addr() {
        let addr: Multiaddr =
            "/ip4/1.2.3.4/udp/4004/quic-v1/webtransport/p2p-circuit/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
                .parse()
                .unwrap();

        assert_eq!(parse_dial(&addr), None);
    }

    /// 非 SHA2-256 的 certhash 必须整条拒绝。放行等于让校验形同虚设：
    /// 后续比对时它永远匹配不上任何一张真证书，却已经把地址判成"合法"。
    ///
    /// ⚠️ **两个用例缺一不可，且第一个才是真正验 code 的那个。** 只用 sha1 写这条时它
    /// 是假绿的：sha1 摘要 20 字节，被 `try_into::<[u8; 32]>` 挡下，把 code 检查整段删掉
    /// 测试照样通过。摘要长度恰为 32 的别的算法（blake3-256）才能把 code 这条判据暴露出来。
    #[test]
    fn rejects_non_sha256_certhash() {
        let socket = "1.2.3.4:4004".parse().unwrap();

        // 0x1e = blake3-256：摘要同为 32 字节，长度检查放行，只有 code 检查能拦住。
        let blake3 = Multihash::<64>::wrap(0x1e, &[0u8; 32]).unwrap();
        assert_eq!(
            parse_dial(&advertise_addr(socket, &[]).with(Protocol::Certhash(blake3))),
            None,
            "摘要长度合法但算法不对的 certhash 必须拒绝"
        );

        // 0x11 = sha1：长度也不对，走的是另一条判据。
        let sha1 = Multihash::<64>::wrap(0x11, &[0u8; 20]).unwrap();
        assert_eq!(
            parse_dial(&advertise_addr(socket, &[]).with(Protocol::Certhash(sha1))),
            None,
            "摘要长度不足 32 的 certhash 必须拒绝"
        );
    }

    #[test]
    fn certhash_multihash_roundtrip() {
        let h = hash(7);
        assert_eq!(Certhash::from_multihash(&h.to_multihash()), Some(h));
    }

    /// 具体地址原样通告，且必须带上全部 certhash。
    #[test]
    fn concrete_addr_is_announced_verbatim() {
        let bound: SocketAddr = "192.168.1.5:4004".parse().unwrap();
        let addrs = announce_addrs(bound, &[hash(1), hash(2)]);

        assert_eq!(addrs, vec![advertise_addr(bound, &[hash(1), hash(2)])]);
    }

    /// 通配地址必须展开：`/ip4/0.0.0.0/...` 通告出去对端根本没法拨。
    #[test]
    fn wildcard_addr_is_expanded_to_interfaces() {
        let addrs = announce_addrs("0.0.0.0:4004".parse().unwrap(), &[hash(1)]);

        assert!(!addrs.is_empty());
        for addr in &addrs {
            match addr.iter().next() {
                Some(Protocol::Ip4(ip)) => {
                    assert!(!ip.is_unspecified(), "通告地址不能是通配符：{addr}")
                }
                other => panic!("应是 IPv4 地址，实得 {other:?}"),
            }
            assert!(
                addr.iter().any(|p| p == Protocol::Udp(4004)),
                "端口必须保留：{addr}"
            );
            assert!(
                addr.iter().any(|p| matches!(p, Protocol::Certhash(_))),
                "必须带 certhash：{addr}"
            );
        }
    }
}
