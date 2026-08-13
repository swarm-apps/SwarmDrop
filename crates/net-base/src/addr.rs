//! 网络地址：[`Addr`]（Multiaddr newtype + 可拨性分类）与 [`NodeAddr`]。
//!
//! 分类谓词迁自旧栈 `libs/core/src/addr.rs`——地址「可拨性/可路由范围」的
//! 判定曾散落在 event loop、infra、presence 三处手写，谓词位运算漂移过一次
//! （IPv6 link-local 漏判），故收口于此，保持单一实现。

use std::fmt;
use std::str::FromStr;

use multiaddr::{Multiaddr, Protocol};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{NodeId, TransportKind};

/// 地址字符串解析失败。
#[derive(Debug, thiserror::Error)]
#[error("invalid multiaddr: {0}")]
pub struct AddrParseError(String);

/// 一条地址值不值得拨——**唯一的传输优劣定义**。
///
/// # 为什么需要它
///
/// libp2p 是**并发拨号**：候选地址一起发出去，谁先建连谁赢。那是一场**延迟**竞赛，
/// 而我们在意的是**吞吐**——两者恰好反向：中继复用一条已建立的连接，建连最快，
/// 吞吐却最差；打洞要等 ICE 收敛数秒，几乎必然输掉竞速。
///
/// 于是「谁赢」这件事必须由**层级**决定，不能交给竞速。层内可以继续竞速（同级之间
/// 差别不大，先连上的就是好的），层间必须有序。
///
/// # 分档依据
///
/// 回环实测（64 MiB × 6 次中位数）：TCP 933 · **WebTransport 322** · QUIC 266 ·
/// **webrtc-direct 72** MiB/s，且 webrtc-direct 的方差大一个数量级（43.7–288）。
/// 前三者同量级、最后一个差 4.5 倍，故切在那里。中继单列——它多一跳，还要吃
/// 中继节点的带宽与 CPU。
///
/// **`Ord` 的方向是「越小越好」**，可以直接 `min()`/排序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DialTier {
    /// 直连 + 高吞吐：TCP / QUIC / WebTransport。
    DirectFast,
    /// 直连 + 低吞吐：webrtc-direct 与 WebRTC 打洞。
    ///
    /// 打洞归这一档而不是中继档：它的 circuit 段只用于**信令**，数据面一个字节不过中继。
    DirectSlow,
    /// 中继：数据面整条经第三方转发。
    Relayed,
}

/// 该段是否是「数据面传输」段。用于判断 `/p2p-circuit` 之后还有没有真正的传输。
fn is_transport_segment(p: &Protocol<'_>) -> bool {
    matches!(
        p,
        Protocol::Tcp(_)
            | Protocol::QuicV1
            | Protocol::Quic
            | Protocol::WebTransport
            | Protocol::WebRTC
            | Protocol::WebRTCDirect
            | Protocol::Ws(_)
            | Protocol::Wss(_)
    )
}

/// 一串段落里的传输档位。**判定顺序按特异性从高到低**，与 [`Addr::transport`] 同源：
/// WebTransport 地址同时含 `/quic-v1`，判反了会把最快的一档降级。
fn tier_of(segments: &[Protocol<'_>]) -> DialTier {
    if segments
        .iter()
        .any(|p| matches!(p, Protocol::WebRTC | Protocol::WebRTCDirect))
    {
        return DialTier::DirectSlow;
    }
    DialTier::DirectFast
}

/// 网络地址（Multiaddr newtype）。
///
/// 字符串表示即 multiaddr 文本格式（`/ip4/192.168.1.2/tcp/4001`），
/// serde 序列化为字符串。libp2p 的 `Multiaddr` 不出内核，上层只见本类型。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Addr(Multiaddr);

impl Addr {
    /// 含 loopback 地址（127.0.0.0/8、::1）。
    pub fn is_loopback(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => ip.is_loopback(),
            Protocol::Ip6(ip) => ip.is_loopback(),
            _ => false,
        })
    }

    /// 含 loopback 或 unspecified 地址（对任何对端都不可拨）。
    pub fn is_loopback_or_unspecified(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => ip.is_loopback() || ip.is_unspecified(),
            Protocol::Ip6(ip) => ip.is_loopback() || ip.is_unspecified(),
            _ => false,
        })
    }

    /// 可路由于局域网的私网地址（IPv4 私网段 / IPv6 ULA fc00::/7），
    /// 排除 loopback/link-local/unspecified。
    pub fn is_private_lan(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => {
                ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
            }
            Protocol::Ip6(ip) => is_v6_ula(&ip) && !ip.is_loopback() && !ip.is_unspecified(),
            _ => false,
        })
    }

    /// 是否位于运营商共享地址空间（100.64.0.0/10）。Tailscale 等 mesh VPN 常用此段，
    /// 它不是公网，即使标准库的 `is_private` 不会把它归为 RFC1918 私网。
    pub fn is_shared_address_space(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => is_v4_shared(&ip),
            _ => false,
        })
    }

    /// 是否为 RFC 2544 基准测试网段（198.18.0.0/15）。它不是互联网可路由地址，
    /// 但某些虚拟网络会显式使用它，邀请筛选应把它作为受限候选而非直接丢弃。
    pub fn is_benchmarking_address(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => is_v4_benchmarking(&ip),
            _ => false,
        })
    }

    /// 公网可路由地址（含 DNS 名）：排除 loopback/unspecified/私网/ULA/link-local、
    /// 共享地址空间与 RFC 2544 基准测试网段。
    pub fn is_public_routable(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => {
                !ip.is_private()
                    && !ip.is_loopback()
                    && !ip.is_link_local()
                    && !ip.is_unspecified()
                    && !is_v4_shared(&ip)
                    && !is_v4_benchmarking(&ip)
            }
            Protocol::Ip6(ip) => {
                !ip.is_loopback()
                    && !ip.is_unspecified()
                    && !is_v6_ula(&ip)
                    && !is_v6_link_local(&ip)
            }
            Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_) => true,
            _ => false,
        })
    }

    /// p2p-circuit 跳数（0=直连地址，1=一跳中继，>1 libp2p 硬拒）。
    pub fn circuit_hops(&self) -> usize {
        self.0
            .iter()
            .filter(|p| matches!(p, Protocol::P2pCircuit))
            .count()
    }

    /// 是否为中继地址（含 p2p-circuit 段）。
    pub fn is_circuit(&self) -> bool {
        self.circuit_hops() > 0
    }

    /// 这条地址属于哪一档「值不值得拨」。判据见 [`DialTier`]。
    ///
    /// # 判定必须看 `/p2p-circuit` 的**位置**，不能只看有没有
    ///
    /// 三种地址都同时含 `/p2p-circuit` 与一个 WebRTC 段，靠「有没有」区分不开：
    ///
    /// | 地址 | 真实身份 | 档位 |
    /// |---|---|---|
    /// | `…/webrtc-direct/certhash/…/p2p/R/p2p-circuit/p2p/T` | **中继**，第一跳恰好是 webrtc-direct | [`Relayed`](DialTier::Relayed) |
    /// | `…/p2p/R/p2p-circuit/webrtc/p2p/T` | **打洞**，circuit 只用于信令，数据面不过中继 | [`DirectSlow`](DialTier::DirectSlow) |
    /// | `…/udp/…/webrtc-direct/certhash/…/p2p/T` | 直连 | [`DirectSlow`](DialTier::DirectSlow) |
    ///
    /// 所以判据是「**最后一个 `/p2p-circuit` 之后**还有没有传输段」：有 ⇒ 那才是真正的
    /// 数据面传输；没有 ⇒ 这条地址就是中继本身。
    ///
    /// 把第一行判成 `DirectSlow` 的后果尤其隐蔽：浏览器的中继地址第一跳正是
    /// webrtc-direct，于是「升级到直连」会把**换一条中继**当成升级完成，从此不再尝试真直连。
    pub fn dial_tier(&self) -> DialTier {
        let segments: Vec<Protocol<'_>> = self.0.iter().collect();
        let after_circuit = segments
            .iter()
            .rposition(|p| matches!(p, Protocol::P2pCircuit))
            .map(|i| &segments[i + 1..]);

        match after_circuit {
            // 非 circuit 地址：整条都是数据面。
            None => tier_of(&segments),
            // circuit 之后还有传输段 = 打洞，数据面是直连的。
            Some(rest) if rest.iter().any(is_transport_segment) => tier_of(rest),
            // circuit 之后只剩 `/p2p/<target>`：这条地址就是中继。
            Some(_) => DialTier::Relayed,
        }
    }

    /// 该地址是否包含 QUIC v1 传输段。
    pub fn is_quic_v1(&self) -> bool {
        self.0.iter().any(|p| p == Protocol::QuicV1)
    }

    /// 该地址是否包含 TCP 传输段。
    pub fn is_tcp(&self) -> bool {
        self.0.iter().any(|p| matches!(p, Protocol::Tcp(_)))
    }

    /// 该地址是否包含浏览器可用的 WebRTC 传输段（直连或 relay circuit）。
    pub fn is_webrtc(&self) -> bool {
        self.0
            .iter()
            .any(|p| matches!(p, Protocol::WebRTC | Protocol::WebRTCDirect))
    }

    /// 该地址是否包含 WebTransport 传输段。
    ///
    /// ⚠️ **它与 [`is_quic_v1`](Self::is_quic_v1) 不互斥**：WebTransport 地址形如
    /// `/ip4/…/udp/…/quic-v1/webtransport/certhash/…`，两个段同时在场，所以
    /// `is_quic_v1()` 对它**也为真**。任何「挑一条 QUIC 出来」的地方都必须显式排除它，
    /// 否则会把一条 WebTransport 地址当成裸 QUIC 交出去 —— 判据错了，但没有任何编译错误。
    /// 同一个陷阱在 [`transport`](Self::transport) 里由「WebTransport 先判」解决。
    pub fn is_webtransport(&self) -> bool {
        self.0.iter().any(|p| p == Protocol::WebTransport)
    }

    /// 把地址里的 IP 段换成 `ip`，其余段原样保留。
    ///
    /// 用途是「监听地址 → 公网地址」：绑在 `0.0.0.0` 的节点通告出去的必须是公网 IP，
    /// 而地址里除 IP 外的部分（端口、传输段、**certhash**）都得逐字保留 —— certhash
    /// 正是那条地址的全部价值，丢一个对端就拨不通。
    ///
    /// 地址里没有 IP 段（如纯 `/dns4/…`）时原样返回。
    pub fn with_ip(&self, ip: std::net::IpAddr) -> Self {
        Self(
            self.0
                .iter()
                .map(|p| match p {
                    Protocol::Ip4(_) | Protocol::Ip6(_) => match ip {
                        std::net::IpAddr::V4(v4) => Protocol::Ip4(v4),
                        std::net::IpAddr::V6(v6) => Protocol::Ip6(v6),
                    },
                    other => other,
                })
                .collect(),
        )
    }

    /// 承载这条地址的传输协议。
    ///
    /// **WebRTC 两个变体必须先判**：打洞地址天生带 circuit 段（信令确实经 relay），
    /// 它的 `/webrtc` 在整条地址的**后半段**，而前半段是到 relay 的 `/tcp` 或
    /// `/quic-v1`——按协议栈顺序找会把打洞连接报成 TCP。同一个陷阱在
    /// `classify_path` 里也有（那里的解法是 `is_hole_punched` 排在 `relayed` 之前）。
    ///
    /// 纯 circuit 地址（无 `/webrtc`）返回的正是**承载中转字节的那条连接**的传输
    /// ——本端 ↔ relay 之间的 TCP/QUIC，这是排障要看的东西。
    ///
    /// 返回 `None` 的一种真实情况：入站中继连接的 `send_back_addr` 只有
    /// `/p2p/<src>` 一段，libp2p 就是这么填的，地址里没有任何传输信息可读。
    /// 呈现层据此显示「未知」，不要编一个默认值。
    pub fn transport(&self) -> Option<TransportKind> {
        if self.0.iter().any(|p| p == Protocol::WebRTC) {
            return Some(TransportKind::Webrtc);
        }
        if self.0.iter().any(|p| p == Protocol::WebRTCDirect) {
            return Some(TransportKind::WebrtcDirect);
        }
        // ⚠️ 必须排在 QuicV1 之前：WebTransport 地址形如 `…/udp/…/quic-v1/webtransport`，
        // 两个段同时存在。漏了这一条它会被判成普通 QUIC，于是上层拿一条 WebTransport
        // 地址去问 libp2p-quic 要连接 —— 判据错了但没有任何编译错误。
        if self.0.iter().any(|p| p == Protocol::WebTransport) {
            return Some(TransportKind::Webtransport);
        }
        self.0.iter().find_map(|p| match p {
            Protocol::QuicV1 => Some(TransportKind::Quic),
            Protocol::Tcp(_) => Some(TransportKind::Tcp),
            _ => None,
        })
    }

    /// 中转身份：circuit 地址里 `/p2p-circuit` **之前**的那个 `/p2p/<id>` 段。
    ///
    /// 与 [`p2p_node_id`](Self::p2p_node_id) 互补——那个取末位（目标身份），
    /// 这个取中转身份。非 circuit 地址返回 `None`。
    pub fn relay_node_id(&self) -> Option<NodeId> {
        let mut last = None;
        for p in self.0.iter() {
            match p {
                Protocol::P2p(peer_id) => last = Some(NodeId::from_peer_id(peer_id)),
                // circuit 段之后的 P2p 是目标，不是中继——就此打住
                Protocol::P2pCircuit => return last,
                _ => {}
            }
        }
        None
    }

    /// 提取地址内嵌的节点身份（`/p2p/<id>` 段）。
    ///
    /// 取**最后一个** P2p 段：circuit 地址 `/…/p2p/RELAY/p2p-circuit/p2p/TARGET`
    /// 里前一个 P2p 是中继身份、目标身份在末位，天真的字符串 `split("/p2p/")`
    /// 会抓到中继而非目标。
    pub fn p2p_node_id(&self) -> Option<NodeId> {
        // multiaddr::Iter 非 DoubleEndedIterator，无法 rev；正向过滤取 last 等效取末位。
        self.0
            .iter()
            .filter_map(|p| match p {
                Protocol::P2p(peer_id) => Some(NodeId::from_peer_id(peer_id)),
                _ => None,
            })
            .last()
    }

    /// multiaddr 二进制字节形态（wire 紧凑编码用——文本形态约有 2x 膨胀）。
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// 从 multiaddr 二进制字节恢复（[`Addr::to_bytes`] 的反向）。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AddrParseError> {
        Multiaddr::try_from(bytes.to_vec())
            .map(Self)
            .map_err(|e| AddrParseError(e.to_string()))
    }

    /// 内核内部互转用，业务层不应调用。
    #[doc(hidden)]
    pub fn from_multiaddr(addr: Multiaddr) -> Self {
        Self(addr)
    }

    /// 内核内部互转用，业务层不应调用。
    #[doc(hidden)]
    pub fn as_multiaddr(&self) -> &Multiaddr {
        &self.0
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // multiaddr 的 Debug 不带引号易与相邻输出粘连（spike 踩过），统一走 Display
        write!(f, "Addr({})", self.0)
    }
}

impl FromStr for Addr {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Multiaddr::from_str(s)
            .map(Self)
            .map_err(|e| AddrParseError(format!("{s}: {e}")))
    }
}

impl Serialize for Addr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Addr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// 节点标识 + 候选地址集。
///
/// `addrs` 为空表示「只知道身份」——内核 `connect` 会走 AddressLookup 解析。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAddr {
    pub id: NodeId,
    pub addrs: Vec<Addr>,
}

impl NodeAddr {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            addrs: Vec::new(),
        }
    }

    pub fn with_addrs(id: NodeId, addrs: Vec<Addr>) -> Self {
        Self { id, addrs }
    }
}

impl From<NodeId> for NodeAddr {
    fn from(id: NodeId) -> Self {
        Self::new(id)
    }
}

fn is_v6_ula(ip: &std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_v6_link_local(ip: &std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn is_v4_shared(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0b1100_0000) == 0b0100_0000
}

fn is_v4_benchmarking(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && (octets[1] & 0b1111_1110) == 18
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> Addr {
        s.parse().unwrap()
    }

    const RELAY: &str = "12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep";
    const TARGET: &str = "12D3KooWMYnFbMsU1dwnPTRcsCHhMHA9MBFxFrCv4puyuiURBaCY";
    // 真实 certhash（取自实测日志）——占位串不是合法 multibase multihash，解析会直接失败。
    const H1: &str = "uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA";
    const H2: &str = "uEiDSOtFQBoepe-LRH2mZPMLHGoMcxnmaM8a02_72my1v9Q";

    /// 直连地址的分档：只看传输，webrtc 系比其余慢一档。
    #[test]
    fn direct_addrs_split_on_the_webrtc_boundary() {
        for fast in [
            "/ip4/192.168.1.5/tcp/4001".to_string(),
            "/ip4/192.168.1.5/udp/4001/quic-v1".to_string(),
            format!("/ip4/192.168.1.5/udp/4004/quic-v1/webtransport/certhash/{H1}/certhash/{H2}"),
        ] {
            assert_eq!(addr(&fast).dial_tier(), DialTier::DirectFast, "{fast}");
        }
        assert_eq!(
            addr(&format!(
                "/ip4/192.168.1.5/udp/4003/webrtc-direct/certhash/{H1}"
            ))
            .dial_tier(),
            DialTier::DirectSlow
        );
    }

    /// **本组是 `dial_tier` 存在的全部理由。** 三条地址都同时含 `/p2p-circuit` 与一个
    /// WebRTC 段，靠「有没有」区分不开，只能看位置。
    #[test]
    fn circuit_is_judged_by_position_not_presence() {
        // ① 浏览器连 bootstrap 的真实形态：第一跳是 webrtc-direct，但这条**是中继**。
        //    判成 DirectSlow 的后果：「升级到直连」会把换一条中继当成升级成功。
        let via_webrtc_direct = format!(
            "/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/{H1}/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}"
        );
        assert_eq!(addr(&via_webrtc_direct).dial_tier(), DialTier::Relayed);

        // ② 打洞：circuit 只用于信令，数据面一个字节不过中继 ⇒ 直连档。
        let hole_punch =
            format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}/p2p-circuit/webrtc/p2p/{TARGET}");
        assert_eq!(addr(&hole_punch).dial_tier(), DialTier::DirectSlow);

        // ③ 朴素中继：circuit 之后只剩 /p2p/<target>。
        let plain = format!("/ip4/47.115.172.218/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}");
        assert_eq!(addr(&plain).dial_tier(), DialTier::Relayed);
    }

    /// 档位的序必须是「越小越好」——上层直接拿它 `min()` 求当前最优路径。
    #[test]
    fn tiers_order_best_first() {
        assert!(DialTier::DirectFast < DialTier::DirectSlow);
        assert!(DialTier::DirectSlow < DialTier::Relayed);
        assert_eq!(
            [
                DialTier::Relayed,
                DialTier::DirectFast,
                DialTier::DirectSlow
            ]
            .into_iter()
            .min(),
            Some(DialTier::DirectFast)
        );
    }

    /// WebTransport 地址同时含 `/quic-v1` 与 `/webtransport`，两者都在 DirectFast，
    /// 所以这条不会因判定顺序出错——但它与 `transport()` 共用「特异性从高到低」这条
    /// 不变量，一起钉住，免得将来把 WebTransport 单独提档时漏改一处。
    #[test]
    fn webtransport_and_quic_agree_between_tier_and_transport() {
        let wt = addr(&format!(
            "/ip4/1.2.3.4/udp/4004/quic-v1/webtransport/certhash/{H1}/certhash/{H2}"
        ));
        assert_eq!(wt.transport(), Some(TransportKind::Webtransport));
        assert_eq!(wt.dial_tier(), DialTier::DirectFast);
    }

    // 迁自 libs/core/src/addr.rs 的分类矩阵，语义必须逐条保持
    #[test]
    fn classification_matrix() {
        // loopback / unspecified
        assert!(addr("/ip4/127.0.0.1/tcp/1").is_loopback());
        assert!(addr("/ip4/0.0.0.0/tcp/1").is_loopback_or_unspecified());
        assert!(!addr("/ip4/192.168.1.2/tcp/1").is_loopback_or_unspecified());

        // 私网 LAN
        assert!(addr("/ip4/192.168.1.2/tcp/1").is_private_lan());
        assert!(addr("/ip6/fd00::1/tcp/1").is_private_lan());
        assert!(!addr("/ip4/127.0.0.1/tcp/1").is_private_lan());
        assert!(!addr("/ip4/8.8.8.8/tcp/1").is_private_lan());

        // 公网
        assert!(addr("/ip4/203.0.113.7/tcp/1").is_public_routable());
        assert!(addr("/dns4/relay.example.com/tcp/1").is_public_routable());
        assert!(addr("/ip4/100.100.200.77/tcp/1").is_shared_address_space());
        assert!(
            !addr("/ip4/100.100.200.77/tcp/1").is_public_routable(),
            "Tailscale 默认共享地址空间不能当公网"
        );
        assert!(addr("/ip4/198.18.0.1/tcp/1").is_benchmarking_address());
        assert!(!addr("/ip4/198.18.0.1/tcp/1").is_public_routable());
        assert!(!addr("/ip4/192.168.1.2/tcp/1").is_public_routable());
        assert!(addr("/ip4/192.168.1.2/tcp/1").is_tcp());
        assert!(addr("/ip4/192.168.1.2/udp/1/quic-v1").is_quic_v1());
        assert!(addr("/ip4/192.168.1.2/udp/1/webrtc-direct").is_webrtc());
        assert!(
            !addr("/ip6/fe80::1/tcp/1").is_public_routable(),
            "IPv6 link-local 不是公网"
        );
        assert!(!addr("/ip6/fd00::1/tcp/1").is_public_routable());

        // circuit 跳数
        assert_eq!(addr("/ip4/1.2.3.4/tcp/1").circuit_hops(), 0);
        assert_eq!(
            addr("/ip4/1.2.3.4/tcp/1/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp/p2p-circuit")
                .circuit_hops(),
            1
        );
    }

    #[test]
    fn addr_serde_is_string() {
        let a = addr("/ip4/192.168.1.2/tcp/4001");
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, "\"/ip4/192.168.1.2/tcp/4001\"");
        let back: Addr = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn p2p_node_id_takes_last_segment_on_circuit() {
        const RELAY: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        const TARGET: &str = "12D3KooWQYhTNQdmr3ArTeUHRYzFg94BKyTkoWBDWez9kSCVe2Xo";

        // 直连地址：取唯一的 /p2p/ 段
        let direct = addr(&format!("/ip4/1.2.3.4/tcp/1/p2p/{TARGET}"));
        assert_eq!(direct.p2p_node_id().unwrap().to_string(), TARGET);

        // circuit 地址：前一个 P2p 是中继身份，目标在末位——必须取最后一个
        let circuit = addr(&format!(
            "/ip4/1.2.3.4/tcp/1/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}"
        ));
        assert_eq!(
            circuit.p2p_node_id().unwrap().to_string(),
            TARGET,
            "circuit 地址应取目标身份而非中继身份"
        );

        // 无 /p2p/ 段：None
        assert!(addr("/ip4/1.2.3.4/tcp/1").p2p_node_id().is_none());
    }

    #[test]
    fn with_ip_replaces_only_the_ip_segment() {
        // certhash 必须逐字保留 —— 它是这条地址的全部价值。
        let listen = addr(
            "/ip4/192.168.1.5/udp/4004/quic-v1/webtransport\
             /certhash/uEiDDq4_xNyDorZBH5AOG-hE3AR6-YMEYDejBWWHZbnYYCQ",
        );

        assert_eq!(
            listen.with_ip("203.0.113.10".parse().unwrap()).to_string(),
            "/ip4/203.0.113.10/udp/4004/quic-v1/webtransport\
             /certhash/uEiDDq4_xNyDorZBH5AOG-hE3AR6-YMEYDejBWWHZbnYYCQ"
        );
    }

    /// 跨地址族改写（双栈机器上很常见）：IPv6 监听 → IPv4 公网。
    #[test]
    fn with_ip_crosses_address_families() {
        assert_eq!(
            addr("/ip6/fe80::1/udp/4004/quic-v1/webtransport")
                .with_ip("203.0.113.10".parse().unwrap())
                .to_string(),
            "/ip4/203.0.113.10/udp/4004/quic-v1/webtransport"
        );
    }

    /// 没有 IP 段的地址原样返回，不该凭空插一个。
    #[test]
    fn with_ip_leaves_ipless_addresses_alone() {
        let dns = addr("/dns4/relay.example.com/tcp/443/wss");
        assert_eq!(dns.with_ip("203.0.113.10".parse().unwrap()), dns);
    }

    #[test]
    fn transport_reads_the_hop_that_actually_carries_bytes() {
        const RELAY: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        const TARGET: &str = "12D3KooWQYhTNQdmr3ArTeUHRYzFg94BKyTkoWBDWez9kSCVe2Xo";

        assert_eq!(
            addr("/ip4/192.168.1.2/tcp/4001").transport(),
            Some(TransportKind::Tcp)
        );
        assert_eq!(
            addr("/ip4/192.168.1.2/udp/4001/quic-v1").transport(),
            Some(TransportKind::Quic)
        );
        assert_eq!(
            addr("/ip4/47.115.172.218/udp/4003/webrtc-direct").transport(),
            Some(TransportKind::WebrtcDirect)
        );
        // WebTransport 地址同时含 `/quic-v1` 与 `/webtransport`。判据必须先看后者 ——
        // 判成普通 QUIC 的话，上层会拿它去问 libp2p-quic 要连接，而那是永远拨不通的。
        assert_eq!(
            addr("/ip4/47.115.172.218/udp/4004/quic-v1/webtransport").transport(),
            Some(TransportKind::Webtransport)
        );
        assert_eq!(
            addr("/ip4/47.115.172.218/udp/4004/quic-v1/webtransport/certhash/uEiDDq4_xNyDorZBH5AOG-hE3AR6-YMEYDejBWWHZbnYYCQ")
                .transport(),
            Some(TransportKind::Webtransport)
        );

        // 纯中继：读到的是本端 ↔ relay 那条连接的传输
        assert_eq!(
            addr(&format!(
                "/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}"
            ))
            .transport(),
            Some(TransportKind::Tcp)
        );

        // 打洞：/webrtc 在 circuit 段之后，前半段的 /tcp 是到 relay 的信令通道。
        // 按协议栈顺序找会报成 Tcp——数据面明明一个字节不过中继。
        assert_eq!(
            addr(&format!(
                "/ip4/1.2.3.4/tcp/4001/p2p/{RELAY}/p2p-circuit/webrtc/p2p/{TARGET}"
            ))
            .transport(),
            Some(TransportKind::Webrtc),
            "打洞地址必须报 webrtc，不能被 circuit 前半段的 tcp 盖掉"
        );

        // 入站中继连接的 send_back_addr：libp2p 只填 /p2p/<src>，无传输信息可读
        assert_eq!(addr(&format!("/p2p/{TARGET}")).transport(), None);
    }

    #[test]
    fn relay_node_id_takes_the_segment_before_circuit() {
        const RELAY: &str = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        const TARGET: &str = "12D3KooWQYhTNQdmr3ArTeUHRYzFg94BKyTkoWBDWez9kSCVe2Xo";

        let circuit = addr(&format!(
            "/ip4/1.2.3.4/tcp/1/p2p/{RELAY}/p2p-circuit/p2p/{TARGET}"
        ));
        assert_eq!(circuit.relay_node_id().unwrap().to_string(), RELAY);
        assert_eq!(
            circuit.p2p_node_id().unwrap().to_string(),
            TARGET,
            "两个方法必须取到不同的段，否则 UI 会把中继当成对端"
        );

        // 直连地址没有中转身份，末位的 /p2p/ 是对端自己
        let direct = addr(&format!("/ip4/1.2.3.4/tcp/1/p2p/{TARGET}"));
        assert!(direct.relay_node_id().is_none());
    }

    #[test]
    fn invalid_addr_is_rejected() {
        assert!("not an addr".parse::<Addr>().is_err());
        assert!(serde_json::from_str::<Addr>("\"/bogus/xyz\"").is_err());
    }

    #[test]
    fn node_addr_from_node_id_has_no_addrs() {
        let id = crate::SecretKey::generate().node_id();
        let na: NodeAddr = id.into();
        assert_eq!(na.id, id);
        assert!(
            na.addrs.is_empty(),
            "只知身份 → 空地址集 → 走 AddressLookup"
        );

        // roundtrip（NodeAddr 进配对码 record 等 JSON 场景）
        let full = NodeAddr::with_addrs(id, vec![addr("/ip4/192.168.1.2/tcp/4001")]);
        let json = serde_json::to_string(&full).unwrap();
        let back: NodeAddr = serde_json::from_str(&json).unwrap();
        assert_eq!(back, full);
    }
}
