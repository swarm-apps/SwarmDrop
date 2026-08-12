//! [`P2pStream`]：绑定对端与协议的双向字节流，以及活跃流配额。
//!
//! 配额机制迁自旧栈 `libs/core/src/data_channel.rs` 的 `ChannelRegistry`：
//! per-peer / per-protocol 计数 + drop 归还的 guard。超限时**显式拒绝并报
//! typed error**（出站 `OpenError::LimitExceeded`、入站 Router 直接 drop 流），
//! 而非依赖底层 muxer 的静默丢弃。

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::{AsyncRead, AsyncWrite};
use libp2p::PeerId;
use swarmdrop_net_base::{NodeId, ProtocolId};

/// 流方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// 本端主动打开。
    Outbound,
    /// 远端打开、本端经 Router 接受。
    Inbound,
}

/// 一条与对端的双向字节流（`AsyncRead + AsyncWrite`）。
///
/// 由 `Endpoint::open`（出站）或 Router 分发（入站）产生。帧编解码与消息
/// 边界由上层协议实现（RPC helper 或业务自定义帧）。drop 时自动归还配额。
pub struct P2pStream {
    remote: NodeId,
    protocol: ProtocolId,
    direction: Direction,
    stream: libp2p::Stream,
    /// 配额 guard，drop 时释放计数。
    _guard: Option<StreamGuard>,
}

impl P2pStream {
    pub(crate) fn new(
        remote: NodeId,
        protocol: ProtocolId,
        direction: Direction,
        stream: libp2p::Stream,
        guard: Option<StreamGuard>,
    ) -> Self {
        Self {
            remote,
            protocol,
            direction,
            stream,
            _guard: guard,
        }
    }

    /// 对端节点。
    ///
    /// 传输层身份即归属证明：数据面协议必须校验
    /// `stream.remote() == session.peer`（取代已删除的应用层加密所隐式
    /// 承担的归属校验，见迁移计划 §XChaCha20 删除的补偿项）。
    pub fn remote(&self) -> NodeId {
        self.remote
    }

    /// 流上的协议。
    pub fn protocol(&self) -> &ProtocolId {
        &self.protocol
    }

    /// 流方向。
    pub fn direction(&self) -> Direction {
        self.direction
    }
}

impl std::fmt::Debug for P2pStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2pStream")
            .field("remote", &self.remote)
            .field("protocol", &self.protocol)
            .field("direction", &self.direction)
            .finish_non_exhaustive()
    }
}

impl AsyncRead for P2pStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for P2pStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_close(cx)
    }
}

/// 活跃流数量限制（迁自旧栈 `DataChannelLimits`，数值不变）。
#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    /// 每个 peer 最大入站流数。
    pub max_inbound_per_peer: usize,
    /// 每个 peer 最大出站流数。
    pub max_outbound_per_peer: usize,
    /// 每个协议最大活跃流数。
    pub max_per_protocol: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_inbound_per_peer: 4,
            max_outbound_per_peer: 4,
            max_per_protocol: 64,
        }
    }
}

/// 活跃流计数登记表（内核内部共享；`Endpoint::open` 与 Router 各持一份 clone）。
#[derive(Debug, Clone)]
pub(crate) struct StreamRegistry {
    limits: StreamLimits,
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default, Debug)]
struct RegistryInner {
    inbound_per_peer: HashMap<PeerId, usize>,
    outbound_per_peer: HashMap<PeerId, usize>,
    per_protocol: HashMap<ProtocolId, usize>,
}

impl StreamRegistry {
    pub(crate) fn new(limits: StreamLimits) -> Self {
        Self {
            limits,
            inner: Arc::new(Mutex::new(RegistryInner::default())),
        }
    }

    /// 该 peer 当前有没有活跃流（收发两向都算）。
    ///
    /// 用途只有一个：**决定能不能安全地关掉一条劣档连接**（升级之后的清理，见
    /// `actor::prune_inferior_conns`）。粒度只到 peer 而不是 connection，是因为
    /// `libp2p-stream` 开流时**随机挑连接**且不回报挑了哪条——「这条流落在哪条连接上」
    /// 从我们这一侧根本观测不到。
    ///
    /// 于是判据只能保守成「该 peer 一条流都没有」：宁可晚一轮再关，也不能关掉一条
    /// 正在传数据的连接。
    pub(crate) fn is_idle(&self, peer: PeerId) -> bool {
        let inner = self.inner.lock().expect("registry lock poisoned");
        !inner.inbound_per_peer.contains_key(&peer) && !inner.outbound_per_peer.contains_key(&peer)
    }

    /// 尝试占用一条流的配额。成功返回 guard（drop 归还），超限返回 `None`。
    pub(crate) fn try_acquire(
        &self,
        peer: PeerId,
        protocol: ProtocolId,
        direction: Direction,
    ) -> Option<StreamGuard> {
        let mut inner = self.inner.lock().expect("registry lock poisoned");
        let per_peer_limit = match direction {
            Direction::Inbound => self.limits.max_inbound_per_peer,
            Direction::Outbound => self.limits.max_outbound_per_peer,
        };
        let per_peer_count = match direction {
            Direction::Inbound => inner.inbound_per_peer.get(&peer).copied().unwrap_or(0),
            Direction::Outbound => inner.outbound_per_peer.get(&peer).copied().unwrap_or(0),
        };
        let per_protocol_count = inner.per_protocol.get(&protocol).copied().unwrap_or(0);
        if per_peer_count >= per_peer_limit || per_protocol_count >= self.limits.max_per_protocol {
            return None;
        }
        match direction {
            Direction::Inbound => *inner.inbound_per_peer.entry(peer).or_default() += 1,
            Direction::Outbound => *inner.outbound_per_peer.entry(peer).or_default() += 1,
        }
        *inner.per_protocol.entry(protocol.clone()).or_default() += 1;
        Some(StreamGuard {
            registry: self.inner.clone(),
            peer,
            protocol,
            direction,
        })
    }
}

/// 流配额 guard，drop 时归还计数。
pub(crate) struct StreamGuard {
    registry: Arc<Mutex<RegistryInner>>,
    peer: PeerId,
    protocol: ProtocolId,
    direction: Direction,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut inner = self.registry.lock().expect("registry lock poisoned");
        let per_peer = match self.direction {
            Direction::Inbound => &mut inner.inbound_per_peer,
            Direction::Outbound => &mut inner.outbound_per_peer,
        };
        if let Some(c) = per_peer.get_mut(&self.peer) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                per_peer.remove(&self.peer);
            }
        }
        if let Some(c) = inner.per_protocol.get_mut(&self.protocol) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                inner.per_protocol.remove(&self.protocol);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer() -> PeerId {
        libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id()
    }

    fn proto() -> ProtocolId {
        ProtocolId::from_static("/test/data/1")
    }

    /// `is_idle` 是「关掉劣档连接」唯一的安全闸——判错的后果是**打断正在传的文件**。
    ///
    /// 收发两向都要算：接收方的数据流是入站的，只看出站会把正在接收的会话判成空闲。
    #[test]
    fn is_idle_covers_both_directions_and_recovers_after_drop() {
        let reg = StreamRegistry::new(StreamLimits::default());
        let peer = test_peer();
        let other = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();

        assert!(reg.is_idle(peer), "没开流时是空闲的");

        let inbound = reg.try_acquire(peer, proto(), Direction::Inbound);
        assert!(
            !reg.is_idle(peer),
            "入站流也算活跃——接收方的数据流正是入站的"
        );
        assert!(reg.is_idle(other), "别的 peer 不受影响");
        drop(inbound);
        assert!(reg.is_idle(peer), "guard 归还后恢复空闲");

        let outbound = reg.try_acquire(peer, proto(), Direction::Outbound);
        assert!(!reg.is_idle(peer));
        drop(outbound);
        assert!(reg.is_idle(peer));
    }

    // 迁自旧栈 data_channel.rs 的配额语义矩阵
    #[test]
    fn registry_enforces_per_peer_and_per_protocol_limits() {
        let reg = StreamRegistry::new(StreamLimits {
            max_inbound_per_peer: 2,
            max_outbound_per_peer: 1,
            max_per_protocol: 64,
        });
        let peer = test_peer();

        let g1 = reg.try_acquire(peer, proto(), Direction::Inbound);
        let g2 = reg.try_acquire(peer, proto(), Direction::Inbound);
        let g3 = reg.try_acquire(peer, proto(), Direction::Inbound);
        assert!(g1.is_some() && g2.is_some());
        assert!(g3.is_none(), "第三条入站流应超出 per-peer 限制");

        // 入站与出站计数彼此独立
        let out1 = reg.try_acquire(peer, proto(), Direction::Outbound);
        let out2 = reg.try_acquire(peer, proto(), Direction::Outbound);
        assert!(out1.is_some());
        assert!(out2.is_none(), "第二条出站流应超出 per-peer 限制");

        // 释放后可再次占用
        drop(g1);
        assert!(reg.try_acquire(peer, proto(), Direction::Inbound).is_some());
    }

    #[test]
    fn per_protocol_limit_shared_across_directions_and_peers() {
        let reg = StreamRegistry::new(StreamLimits {
            max_inbound_per_peer: 10,
            max_outbound_per_peer: 10,
            max_per_protocol: 2,
        });
        let g1 = reg.try_acquire(test_peer(), proto(), Direction::Inbound);
        let g2 = reg.try_acquire(test_peer(), proto(), Direction::Outbound);
        let g3 = reg.try_acquire(test_peer(), proto(), Direction::Inbound);
        assert!(g1.is_some() && g2.is_some());
        assert!(g3.is_none(), "per-protocol 限制统计所有方向与 peer 的总数");
    }

    #[test]
    fn drop_guard_removes_empty_counters() {
        let reg = StreamRegistry::new(StreamLimits::default());
        let peer = test_peer();
        let g = reg
            .try_acquire(peer, proto(), Direction::Inbound)
            .expect("first acquire");
        drop(g);
        let inner = reg.inner.lock().unwrap();
        assert!(!inner.inbound_per_peer.contains_key(&peer));
        assert!(!inner.per_protocol.contains_key(&proto()));
    }
}
