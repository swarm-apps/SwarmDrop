//! 跨 IPC/FFI 的网络状态枚举。
//!
//! 这些枚举会经 serde 进前端与移动端，命名风格与旧栈 `NodeEvent` 的
//! `#[serde(rename_all = "camelCase")]` 保持一致。

use serde::{Deserialize, Serialize};

/// NAT 状态（AutoNAT v2 探测结论）。
///
/// 刻意没有 `Private` 变体：AutoNAT v2 单次失败不足以判定 Private，
/// 旧栈同款语义。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NatStatus {
    /// 公网可达（AutoNAT 确认）。
    Public,
    /// 未知（未探测完成或探测失败）。
    #[default]
    Unknown,
}

/// 到某个对端的连接路径（产品层只需理解的少数稳定状态，
/// `why-libp2p-not-iroh.md` 的连接路径模型；`Offline` 是 presence 层
/// 状态而非连接路径，不在此枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    /// 局域网直连。
    Local,
    /// 跨网络直连（打洞成功或公网直拨）。
    Direct,
    /// 经 circuit relay 中继。
    Relayed,
}

/// 承载连接的传输协议（由 multiaddr 协议栈判定，见 [`Addr::transport`](crate::Addr::transport)）。
///
/// 与 [`PathKind`] **正交**，排障时要一起看：`PathKind` 回答「字节走不走中继」，
/// 本枚举回答「字节跑在哪种传输上」。同是 `Relayed`，底层 TCP 还是 QUIC 指向
/// 完全不同的排查方向；同是 `Direct`，`Webrtc` 说明是打洞来的、`Quic` 说明是公网直拨。
///
/// 变体名刻意不写成 `WebRtc`：camelCase 序列化后会变成 `"webRtc"`，而三端 UI 与
/// 日志里这个词一直是 `webrtc`。
// `Hash`：内核按传输给候选地址分组时要拿它当 map 键（`actor.rs` 的 `lan_candidates`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    /// TCP（上叠 Noise + Yamux）。
    Tcp,
    /// QUIC v1（传输层自带 TLS）。
    Quic,
    /// WebRTC 打洞：**信令**经 relay，数据面一个字节不过中继。
    Webrtc,
    /// WebRTC Direct：certhash 免信令免域名，浏览器够到原生端的唯一入口。
    WebrtcDirect,
}

/// 地址的发现来源（AddressBook 与 `NetEvent::Discovered` 用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverySource {
    /// mDNS 局域网发现。
    Mdns,
    /// Kademlia DHT。
    Dht,
    /// identify 协议对端上报。
    Identify,
    /// 手动注入（配对码 record、用户配置等）。
    Manual,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 这些枚举经 serde 进前端与移动端——camelCase 表示是 IPC 契约，
    // 改动会静默破坏前端 match，固化在此。
    #[test]
    fn serde_reprs_are_camel_case_ipc_contract() {
        assert_eq!(
            serde_json::to_string(&NatStatus::Public).unwrap(),
            "\"public\""
        );
        assert_eq!(
            serde_json::to_string(&NatStatus::Unknown).unwrap(),
            "\"unknown\""
        );
        assert_eq!(
            serde_json::to_string(&PathKind::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&PathKind::Direct).unwrap(),
            "\"direct\""
        );
        assert_eq!(
            serde_json::to_string(&PathKind::Relayed).unwrap(),
            "\"relayed\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Mdns).unwrap(),
            "\"mdns\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Dht).unwrap(),
            "\"dht\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Identify).unwrap(),
            "\"identify\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Manual).unwrap(),
            "\"manual\""
        );
        assert_eq!(
            serde_json::to_string(&TransportKind::Tcp).unwrap(),
            "\"tcp\""
        );
        assert_eq!(
            serde_json::to_string(&TransportKind::Quic).unwrap(),
            "\"quic\""
        );
        assert_eq!(
            serde_json::to_string(&TransportKind::Webrtc).unwrap(),
            "\"webrtc\"",
            "变体名写成 WebRtc 会序列化成 webRtc，与三端 UI/日志用词不一致"
        );
        assert_eq!(
            serde_json::to_string(&TransportKind::WebrtcDirect).unwrap(),
            "\"webrtcDirect\""
        );
    }

    #[test]
    fn nat_status_defaults_to_unknown() {
        assert_eq!(NatStatus::default(), NatStatus::Unknown);
    }
}
