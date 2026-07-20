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
    }

    #[test]
    fn nat_status_defaults_to_unknown() {
        assert_eq!(NatStatus::default(), NatStatus::Unknown);
    }
}
