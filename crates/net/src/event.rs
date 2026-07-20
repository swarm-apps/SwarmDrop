//! 必达边沿事件：[`NetEvent`] 与订阅句柄 [`Events`]。
//!
//! 与 [`Watcher`](crate::Watcher)（状态采样）互补：这里的每个事件都是
//! 「发生过一次」的边沿语义，经 bounded mpsc 送达——队列满时丢弃并计数
//! （唯一重消费者是 swarmdrop-core 事件循环，轻处理后转 EventBus，
//! 深度 256 足够；presence 另有 watch_conns 差分兜底丢失边沿）。
//!
//! 旧栈巨型 `NodeEvent`（20+ 变体、直接 serde 进前端）拆解后的去向：
//! - `Listening`/`NatStatusChanged`/`RelayServer*`/`LanHelperStatusChanged`
//!   → `watch_addrs` / `watch_nat` / `watch_relays`（状态而非事件）；
//! - `InboundRequest` → 消失（业务入站由 Router 的 ProtocolHandler 吸收）；
//! - 其余保留为本枚举的必达边沿。前端事件由 swarmdrop-core 独立定义，
//!   本枚举不直接进 IPC。

use std::time::Duration;

use swarmdrop_net_base::{Addr, DiscoverySource, NodeId, PathKind, ProtocolId};

/// 内核必达边沿事件。
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// 与对端建立了第一条连接。
    PeerConnected { node: NodeId, path: PathKind },
    /// 与对端的最后一条连接断开。
    PeerDisconnected { node: NodeId },
    /// 连接路径变化（如 dcutr 打洞成功：Relayed → Direct）。
    PathChanged { node: NodeId, path: PathKind },
    /// identify 交换完成（agent/protocol 版本、对端地址、支持的协议）。
    PeerIdentified {
        node: NodeId,
        agent: String,
        protocol: String,
        addrs: Vec<Addr>,
        protocols: Vec<ProtocolId>,
    },
    /// 发现新对端（mDNS / DHT / 手动注入）。
    Discovered {
        node: NodeId,
        addrs: Vec<Addr>,
        source: DiscoverySource,
    },
    /// ping 往返成功（RTT 采样）。
    PingSuccess { node: NodeId, rtt: Duration },
    /// ping 失败（presence 的死对端判定依赖此事件）。
    PingFailure { node: NodeId, error: String },
    /// relay reservation 被接受（`renewal` 为 true 表示续约）。
    RelayReservationAccepted { relay: NodeId, renewal: bool },
    /// relay reservation 失效（circuit listener 关闭）。
    RelayReservationLost { relay: NodeId },
}

/// 事件订阅句柄（`Endpoint::subscribe` 返回）。
#[derive(Debug)]
pub struct Events {
    rx: tokio::sync::mpsc::Receiver<NetEvent>,
}

impl Events {
    pub(crate) fn new(rx: tokio::sync::mpsc::Receiver<NetEvent>) -> Self {
        Self { rx }
    }

    /// 接收下一个事件；Endpoint 关闭后返回 `None`。
    pub async fn recv(&mut self) -> Option<NetEvent> {
        self.rx.recv().await
    }
}
