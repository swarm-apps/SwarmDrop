//! Transport ↔ Behaviour 之间的内部通道。
//!
//! # 为什么需要它
//!
//! libp2p 把「拨号」与「协议流」放在两个互不相通的平面：`Transport` 负责建连接，
//! `NetworkBehaviour` 负责在**已有连接上**开协议流。而本传输的建连过程恰恰需要**先有
//! 一条已建立的（relay）连接**，在其上跑 `/webrtc-signaling/0.0.1` 换 SDP。
//!
//! 于是 `Transport::dial` 做不到自给自足——它必须请 behaviour 代劳开流。两者通过本模块
//! 的一对 channel 配对协作，这也是 [`crate::new`] 必须同时返回两者、且必须注册进**同一个**
//! Swarm 的原因。
//!
//! DCUtR 与上游 PR #5978 面对同一约束，采用的是同一类做法。

use futures::channel::{mpsc, oneshot};
use libp2p_core::Multiaddr;
use libp2p_identity::PeerId;

use crate::Connection;
use crate::error::Error;

/// 通道容量。信令请求是低频事件（每次拨号一条），给足余量即可。
pub(crate) const CHANNEL_CAPACITY: usize = 32;

/// Transport → Behaviour。
#[derive(Debug)]
pub(crate) enum ToBehaviour {
    /// 请求对 `target` 发起信令。
    ///
    /// `signaling_addr` 是**剔除 `/webrtc` 段后**的地址（见 [`crate::addr::split`]），
    /// behaviour 用它确保存在一条可用于开流的连接。
    Dial {
        target: PeerId,
        signaling_addr: Multiaddr,
        /// 建连结果的回送端。dial future 在此等待。
        result: oneshot::Sender<Result<Connection, Error>>,
    },
}

/// Behaviour → Transport。
#[derive(Debug)]
pub(crate) enum ToTransport {
    /// 入站信令完成，得到一条连接。
    ///
    /// **对称性所在**：没有这条，本端就只能主动拨、不能被拨，spec 步骤 4 的 MUST
    /// 就落空了（见 crate 文档）。
    ///
    /// 消费侧（`Transport::poll` → `TransportEvent::Incoming`）已就位，生产侧要等
    /// handler 能受理入站信令流才接上——**这个 allow 就是「入站半条路未通」的标记，
    /// 接线时应随之删除**。
    #[allow(dead_code, reason = "待 handler 落地后由入站信令构造")]
    Incoming {
        peer: PeerId,
        connection: Connection,
    },
}

/// 建立配对的通道端点。
pub(crate) fn pair() -> (TransportSide, BehaviourSide) {
    let (to_behaviour_tx, to_behaviour_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (to_transport_tx, to_transport_rx) = mpsc::channel(CHANNEL_CAPACITY);
    (
        TransportSide {
            to_behaviour: to_behaviour_tx,
            from_behaviour: to_transport_rx,
        },
        BehaviourSide {
            from_transport: to_behaviour_rx,
            to_transport: to_transport_tx,
        },
    )
}

/// Transport 持有的一端。
#[derive(Debug)]
pub(crate) struct TransportSide {
    pub(crate) to_behaviour: mpsc::Sender<ToBehaviour>,
    pub(crate) from_behaviour: mpsc::Receiver<ToTransport>,
}

/// Behaviour 持有的一端。
#[derive(Debug)]
pub(crate) struct BehaviourSide {
    pub(crate) from_transport: mpsc::Receiver<ToBehaviour>,
    #[allow(dead_code, reason = "待 handler 落地后由入站信令使用")]
    pub(crate) to_transport: mpsc::Sender<ToTransport>,
}
