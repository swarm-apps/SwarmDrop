//! 跨 IPC/FFI 的网络状态枚举。
//!
//! 这些枚举会经 serde 进前端与移动端，命名风格与旧栈 `NodeEvent` 的
//! `#[serde(rename_all = "camelCase")]` 保持一致。

use serde::{Deserialize, Serialize};

/// NAT 状态（AutoNAT v2 探测结论）。
///
/// 刻意没有 `Private` 变体：AutoNAT v2 单次失败不足以判定 Private，
/// 旧栈同款语义。
// `specta::Type`：让它经 tauri-specta 导出成 TS 字面量联合（`"public" | "unknown"`）。
// 此前桌面侧在 `NetworkStatus` 上挂 `specta(type = String)` 把它抹成 `string`，
// 移动侧更是 `format!("{:?}")` 出 `"Public"`——两端 UI 都写 `=== "public"`，
// 于是移动端的 NAT 格从有这功能起就恒显示「未知」。类型化后这类错配编译期就露。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
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
///
/// # [`Direct`](Self::Direct) 与 [`HolePunched`](Self::HolePunched) 分列，因为**只有内核看得见**
///
/// 两者的数据面质量相同（一个字节都不过中继），分开只为回答「怎么建起来的」——
/// 而这两条的排查方向相反：打洞说明 NAT 穿透成功，该去看 ICE 与信令；直连说明压根
/// 没打洞，该去看那条地址是谁给的、隧道底下又是什么。
///
/// 这一位曾被折叠：`classify_path` 先用 `is_hole_punched` 判出打洞，再把结果和公网直拨
/// 一起塞进 `Direct`，于是产品层只能从**正交的**传输轴上反推（`TransportKind == Webrtc`
/// ⇒ 打洞）。那条反推依赖一条无人看守的谓词等价关系，且看不见下面那个缺口。现在结论
/// 由内核直接给出，产品层一对一映射。
///
/// ⚠️ **已知缺口：libp2p 自己的 DCUtR 打洞目前归 [`Direct`](Self::Direct)。**
/// 原生端 `dcutr` behaviour 是开着的（`presets.rs` 的 `Native`），它走 TCP/QUIC 直连，
/// 与 webrtc-p2p 的 ICE 覆盖不同的 NAT 类型、互为补充。但它成功后产出的地址是一条普通
/// `/ip4/<公网>/udp/<port>/quic-v1`，地址上认不出来 —— 要认出它得接
/// `dcutr::Event::DirectConnectionUpgradeSucceeded`（actor 目前把它落进
/// `other => debug!`）。所以本枚举当前只识别 webrtc-p2p 那一条打洞路径。
/// **这是缺口不是判据**：不要据此写下「打洞只可能跑在 WebRTC 上」——那句话是错的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    /// 局域网直连。
    Local,
    /// 跨网络直连，且**不是打洞来的**：拨通了对端自报的地址（公网 IP，或
    /// Tailscale 之类 mesh VPN 的隧道地址）。
    Direct,
    /// 打洞建立的直连：**信令**经 relay，数据面一个字节不过中继。
    ///
    /// 判据是 `is_hole_punched`（地址里 circuit 段之后还有 `/webrtc`），见枚举文档的缺口说明。
    HolePunched,
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
    /// WebTransport（HTTP/3 over QUIC）：与 WebRTC Direct 同样是「certhash 免域名」，
    /// 但数据面就是 QUIC —— 没有 ICE/DTLS/SCTP 三层的转发开销。
    Webtransport,
}

impl TransportKind {
    /// 跨 IPC / FFI 的 wire 名，**与 serde 的 camelCase 逐字一致**。
    ///
    /// 存在的理由是它此前有三份手抄副本（core 的地址校验、移动端 FFI 镜像、serde 本身），
    /// 加一个传输要同时改三处：漏掉移动端那份的表现是 UI 显示一个陌生的传输名，漏掉校验
    /// 那份是「本端支持」列表里少一项 —— 两者都编得过、都没有测试拦。
    ///
    /// 用 `match` 而非 `default` 分支：加变体时这里**编译失败**，那正是要的。
    /// 与 serde 的一致性由 `wire_names_match_serde` 钉死。
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Quic => "quic",
            Self::Webrtc => "webrtc",
            Self::WebrtcDirect => "webrtcDirect",
            Self::Webtransport => "webtransport",
        }
    }
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
            serde_json::to_string(&TransportKind::WebrtcDirect).unwrap(),
            "\"webrtcDirect\""
        );
        assert_eq!(
            serde_json::to_string(&TransportKind::Webtransport).unwrap(),
            "\"webtransport\""
        );
        assert_eq!(
            serde_json::to_string(&PathKind::Relayed).unwrap(),
            "\"relayed\""
        );
    }

    /// [`TransportKind::wire_name`] 必须与 serde 的 camelCase 逐字一致。
    ///
    /// 两者是同一条 wire 契约的两个入口（serde 给 IPC，`wire_name` 给 FFI 与地址校验），
    /// 漂移的表现是同一个传输在不同界面显示成两个名字。**加变体时这条测试会因为 match
    /// 不穷尽而编译失败** —— 那正是它的第二个作用。
    #[test]
    fn wire_names_match_serde() {
        for kind in [
            TransportKind::Tcp,
            TransportKind::Quic,
            TransportKind::Webrtc,
            TransportKind::WebrtcDirect,
            TransportKind::Webtransport,
        ] {
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.wire_name()),
                "{kind:?} 的 wire_name 与 serde 表示不一致"
            );
        }
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
