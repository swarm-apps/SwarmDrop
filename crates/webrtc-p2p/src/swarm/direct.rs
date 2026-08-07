//! direct 模式在 swarm 平面的共享类型。
//!
//! 两个 target 的 `DirectTransport`（[`native`] 能监听也能拨，[`wasm`] 只能拨）产出
//! 同一种事件，于是 [`crate::swarm::transport`] 只需一处 `cfg` 选实现，事件处理代码
//! 两端共用。
//!
//! 事件类型直接用 [`libp2p_core::transport::TransportEvent`]——曾经在这里放过一个
//! 逐字段同构的 `DirectEvent`，那只是把同一份数据拷来拷去，两侧的 backend 本来就
//! 已经依赖 `libp2p-core` 了。
//!
//! [`native`]: crate::backend::native::direct::transport
//! [`wasm`]: crate::backend::wasm::direct

use futures::StreamExt;
use futures::channel::mpsc;
use futures::future::BoxFuture;
use libp2p_core::transport::TransportEvent;
use libp2p_identity::PeerId;

use crate::error::Error;
use crate::swarm::connection::Connection;

/// 一次 direct 建连的结果。
pub(crate) type Upgrade = BoxFuture<'static, Result<(PeerId, Connection), Error>>;

/// direct 传输平面产出的事件。
pub(crate) type DirectEvent = TransportEvent<Upgrade, Error>;

/// `PeerConnection` 连接状态的**终态**。
///
/// 中间态（`connecting` / `new`）在转发时就滤掉了——调用方只关心成没成。
/// `Disconnected` 也不算终态，它可能自行恢复。
#[derive(Debug)]
pub(crate) enum StateEvent {
    Connected,
    Failed,
}

/// 等 DTLS 握手完成。
///
/// 只取第一个事件就够——[`StateEvent`] 的两个变体都是终态。
///
/// **别省掉这一步。** 少了它，失败的连接只表现为「Noise 握手永远不返回」，最后由
/// 上层超时收场——拿不到任何原因。两个 target 的接收侧完全一致（发送侧才有平台差异），
/// 故放在这里共用。
pub(crate) async fn await_connected(
    states: &mut mpsc::UnboundedReceiver<StateEvent>,
) -> Result<(), Error> {
    match states.next().await {
        Some(StateEvent::Connected) => Ok(()),
        Some(StateEvent::Failed) => Err(Error::Connection("WebRTC 连接进入 failed 状态".into())),
        None => Err(Error::Connection("PeerConnection 在建连前已关闭".into())),
    }
}
