//! 网络地址：[`Addr`]（Multiaddr newtype + 可拨性分类）与 [`NodeAddr`]。
//!
//! 分类谓词迁自旧栈 `libs/core/src/addr.rs`——地址「可拨性/可路由范围」的
//! 判定曾散落在 event loop、infra、presence 三处手写，谓词位运算漂移过一次
//! （IPv6 link-local 漏判），故收口于此，保持单一实现。

use std::fmt;
use std::str::FromStr;

use multiaddr::{Multiaddr, Protocol};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::NodeId;

/// 地址字符串解析失败。
#[derive(Debug, thiserror::Error)]
#[error("invalid multiaddr: {0}")]
pub struct AddrParseError(String);

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

    /// 公网可路由地址（含 DNS 名）：排除 loopback/unspecified/私网/ULA/link-local。
    pub fn is_public_routable(&self) -> bool {
        self.0.iter().any(|p| match p {
            Protocol::Ip4(ip) => {
                !ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> Addr {
        s.parse().unwrap()
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
        assert!(!addr("/ip4/192.168.1.2/tcp/1").is_public_routable());
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
