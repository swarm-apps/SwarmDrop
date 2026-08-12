//! 数据面：把一条 WebTransport 会话暴露成 libp2p 的 [`StreamMuxer`]。
//!
//! 这是全 crate 最薄的一层，因为 **QUIC 的流本身就是流**：一条 bidi 流直接就是一条
//! libp2p 子流，没有 framing、没有标签、没有额外的握手消息。`webrtc-p2p` 那边的三个坑
//! （DataChannel 的 `init` 通道、per-message framing、`ordered` 默认值）在这里一个都不存在。
//!
//! 唯一的适配是 async 方言：`wtransport` 的流实现 **tokio** 的 `AsyncRead`/`AsyncWrite`，
//! libp2p 要 **futures** 的，靠 `tokio_util::compat` 转。
//!
//! # 为什么 `Drop` 里要显式关闭会话
//!
//! `poll_close` 只在 libp2p 走**正常**关闭流程时才被调到。对端掉线、Swarm 直接丢弃连接、
//! 建连中途被 `ConcurrentDial` 取消 —— 这些路径上 muxer 是被直接 drop 的。
//! `webrtc-p2p` 为此吃过一次亏（泄漏的 driver 任务把 CPU 跑到 718%），这里从一开始就
//! 用 RAII 收口：**muxer 是数据面的最后持有者，它一死会话就必须关**。

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, FutureExt};
use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use wtransport::{Connection, RecvStream, SendStream, VarInt};

use crate::error::Error;

/// 正常关闭时发给对端的 QUIC application error code。
///
/// 0 = 无错误。libp2p 没有为传输层定义关闭码，用 0 表达「这是一次有序关闭」。
const CLOSE_OK: VarInt = VarInt::from_u32(0);

/// 一条 libp2p 子流 = 一条 WebTransport bidi 流。
///
/// 读写两半各自独立（QUIC 的流本就是单向一对），所以不是简单的 `Compat<BiStream>`：
/// 分开持有让 `poll_close` 只关写半边而不影响仍在读的那半，与 libp2p 的半关闭语义一致。
pub struct Substream {
    send: Compat<SendStream>,
    recv: Compat<RecvStream>,
}

impl Substream {
    pub(crate) fn new((send, recv): (SendStream, RecvStream)) -> Self {
        Self {
            send: send.compat_write(),
            recv: recv.compat(),
        }
    }
}

impl AsyncRead for Substream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for Substream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.send).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.send).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.send).poll_close(cx)
    }
}

/// 一条 WebTransport 会话的数据面。
pub struct Muxer {
    session: Connection,
    /// 进行中的 `accept_bi`。
    ///
    /// `wtransport` 只提供 async 方法，而 `StreamMuxer` 是 poll 风格，所以把「取下一条流」
    /// 这件事存成一个 future。`Connection` 是 `Clone` 的（内部 `Arc`），future 因此不必
    /// 借用 `self`，也就不存在自引用结构。
    inbound: Option<BoxFuture<'static, Result<(SendStream, RecvStream), Error>>>,
    outbound: Option<BoxFuture<'static, Result<(SendStream, RecvStream), Error>>>,
    /// 拨号方向那个临时的 client endpoint，**只为保活**。
    ///
    /// wtransport 把 TLS 配置挂在 `Endpoint` 上，而每次拨号愿意接受的 certhash 集合都不同
    /// —— 所以拨号必须各起一个 endpoint，没法复用一个共享的。它得活到连接结束：quinn 的
    /// endpoint handle 全部释放后，其 driver 任务的存续就取决于上游实现细节，与其赌那个，
    /// 不如显式持有到 muxer 一起死。监听方向为 `None`（endpoint 归 `Listener`）。
    _client_endpoint: Option<wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>>,
}

impl Muxer {
    pub(crate) fn new(
        session: Connection,
        client_endpoint: Option<wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>>,
    ) -> Self {
        Self {
            session,
            inbound: None,
            outbound: None,
            _client_endpoint: client_endpoint,
        }
    }
}

impl StreamMuxer for Muxer {
    type Substream = Substream;
    type Error = Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        // ⚠️ **不能写成 `get_or_insert_with(|| { … session.clone() … })`**：那样克隆会在
        // 闭包之外无条件发生，future 已存在时被捕获后原样丢弃。`Connection` 的 clone/drop
        // 是 6 次原子 RMW（`quinn::ConnectionRef` 的引用计数 + 两个 `Arc`），而 libp2p-swarm
        // 的连接循环**每一轮**都调 `poll_inbound` —— 跑满数据面时每秒数千到数万次。
        if this.inbound.is_none() {
            let session = this.session.clone();
            this.inbound = Some(
                async move {
                    session
                        .accept_bi()
                        .await
                        .map_err(|e| Error::Connect(format!("接受入站子流失败：{e}")))
                }
                .boxed(),
            );
        }
        let fut = this.inbound.as_mut().expect("上一行刚填过");

        let result = ready!(fut.poll_unpin(cx));
        this.inbound = None;
        Poll::Ready(result.map(Substream::new))
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        // 同 `poll_inbound`：克隆只在真正要建 future 时发生。
        if this.outbound.is_none() {
            let session = this.session.clone();
            this.outbound = Some(
                async move {
                    // 两段 await：先向对端申请开流（可能被流控挡住），再等流真正就绪。
                    session
                        .open_bi()
                        .await
                        .map_err(|e| Error::Connect(format!("申请出站子流失败：{e}")))?
                        .await
                        .map_err(|e| Error::Connect(format!("打开出站子流失败：{e}")))
                }
                .boxed(),
            );
        }
        let fut = this.outbound.as_mut().expect("上一行刚填过");

        let result = ready!(fut.poll_unpin(cx));
        this.outbound = None;
        Poll::Ready(result.map(Substream::new))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `close` 是同步的：它只是把关闭帧排进发送队列。没有可等待的完成态，
        // 因此直接 Ready 而不是挂起等确认 —— QUIC 的关闭本来就是尽力而为。
        self.session.close(CLOSE_OK, b"");
        Poll::Ready(Ok(()))
    }

    fn poll(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        // `StreamMuxerEvent` 目前只有 `AddressChange` 一个变体，用于表达连接迁移到了新的
        // 远端地址。QUIC 确实支持连接迁移，但 `wtransport` 不暴露迁移通知，而误报一次
        // 地址变更会让上层把一条好连接的地址改错。观测不到就不报。
        Poll::Pending
    }
}

impl Drop for Muxer {
    /// 见模块文档：`poll_close` 覆盖不了异常终止与取消两条路径。
    fn drop(&mut self) {
        self.session.close(CLOSE_OK, b"");
    }
}

impl std::fmt::Debug for Muxer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Muxer")
            .field("remote", &self.session.remote_address())
            .field("session_id", &self.session.session_id())
            .finish()
    }
}
