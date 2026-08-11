//! `DataChannel` → `AsyncRead + AsyncWrite` 适配。
//!
//! # 为什么需要它
//!
//! webrtc-rs 0.20 的 `DataChannel` 是**消息式**的（`send(BytesMut)` /
//! `poll() -> DataChannelEvent`），而 libp2p 的 `Stream` 要的是**字节流**。
//! 0.17 曾自带 `PollDataChannel`，0.20 移除了，故这里自己补上。
//!
//! 转换后再套 `libp2p_webrtc_utils::Stream<T>`，由它在字节流之上做 libp2p 的消息
//! framing（半关闭语义等）。「消息 → 字节流 → 再 framing」看着绕了一圈，但那是
//! libp2p-webrtc-utils 的既定设计，官方 `webrtc-websys` 也是这么套的。
//!
//! # 两条约束决定了实现形状
//!
//! 1. **跨 poll 保持 future**：`dc.poll()` 与 `dc.send()` 都是 `async fn`，而
//!    `AsyncRead`/`AsyncWrite` 是 poll 接口。每次被 poll 都新建 future 会丢事件
//!    （上一个在 await 中途被 drop），故进行中的 future 必须存下来跨轮复用。
//! 2. **必须 `Clone`**：`Stream<T>` 要求 `T: AsyncRead + AsyncWrite + Unpin + Clone`
//!    ——它内部要分别交给 framed 编解码与 drop listener。因此可变状态收进
//!    `Arc<Mutex<_>>`，clone 出来的副本**共享同一条底层通道**（这正是所需语义）。
//!
//! 锁用同步 `Mutex`：临界区内只做 poll，从不 await，因此不会阻塞执行器。

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};

use bytes::BytesMut;
use futures::future::BoxFuture;
use futures::{AsyncRead, AsyncWrite, FutureExt};
use rtc::data_channel::RTCDataChannelState;
use webrtc::data_channel::{DataChannel, DataChannelEvent};

/// 把消息式 `DataChannel` 包装成字节流。
///
/// `Clone` 语义是「共享同一条通道」，不是复制。
#[derive(Clone)]
pub(crate) struct PollDataChannel {
    dc: Arc<dyn DataChannel>,
    shared: Arc<Mutex<Shared>>,
}

#[derive(Default)]
struct Shared {
    /// 已收到但尚未被读走的字节。
    read_buf: BytesMut,
    /// 进行中的 `dc.poll()`。
    reading: Option<BoxFuture<'static, Option<DataChannelEvent>>>,
    /// 进行中的 `dc.send()`。
    writing: Option<BoxFuture<'static, Result<(), webrtc::error::Error>>>,
    /// 进行中的 `dc.close()`。
    closing: Option<BoxFuture<'static, Result<(), webrtc::error::Error>>>,
    /// 对端已关闭，读侧到达 EOF。
    eof: bool,
}

impl PollDataChannel {
    pub(crate) fn new(dc: Arc<dyn DataChannel>) -> Self {
        Self {
            dc,
            shared: Arc::new(Mutex::new(Shared::default())),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        // 临界区内只 poll 不 await，唯一的中毒来源是 panic；此时继续用旧状态没有意义，
        // 直接取内层数据继续，避免把一次 panic 放大成全链路不可用。
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 等待进行中的写完成。
fn poll_pending_write(shared: &mut Shared, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    let Some(fut) = shared.writing.as_mut() else {
        return Poll::Ready(Ok(()));
    };
    let result = ready!(fut.poll_unpin(cx));
    shared.writing = None;
    Poll::Ready(result.map_err(to_io))
}

impl AsyncRead for PollDataChannel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let dc = self.dc.clone();
        let mut shared = self.lock();
        // **一次 poll 要尽量填满 `buf`，不能取到一条消息就返回。**
        //
        // 消费速率直接决定 driver 那条队列会不会溢出——它满了就**丢弃**已经可靠送达的
        // 消息（webrtc#858，判据与量化见 `super::sctp_receive_buffer`）。早先每次只搬一条：
        // 上层给 256 KiB 的 buf、一条消息 8 KiB，31/32 的容量空着，每 8 KiB 都要穿一遍
        // 完整的 poll 链。回环实测填满 buf 后吞吐 **2.1×**（145 → 301 MiB/s）。
        //
        // ⚠️ 省下的是 poll 链与 waker 往返，**不是堆分配**：下面每取到一条消息就把
        // `reading` 置空，所以仍是每条消息各 `Box::pin` 一个 `dc.poll()` future。
        // 真要省那个，得让 future 跨消息存活（或换成不装箱的具名 future）。
        let mut filled = 0usize;
        loop {
            if !shared.read_buf.is_empty() {
                let n = (buf.len() - filled).min(shared.read_buf.len());
                buf[filled..filled + n].copy_from_slice(&shared.read_buf.split_to(n));
                filled += n;
                if filled == buf.len() {
                    return Poll::Ready(Ok(filled));
                }
            }
            if shared.eof {
                // 0 字节即 EOF，这是 AsyncRead 的约定；已搬到的字节要先交出去，
                // 否则最后一批数据会连同 EOF 一起丢掉。
                return Poll::Ready(Ok(filled));
            }

            if shared.reading.is_none() {
                let dc = dc.clone();
                shared.reading = Some(async move { dc.poll().await }.boxed());
            }
            let fut = shared.reading.as_mut().expect("刚置入");
            match fut.poll_unpin(cx) {
                Poll::Ready(Some(DataChannelEvent::OnMessage(msg))) => {
                    shared.reading = None;
                    // **接管所有权，别 clone**：`BytesMut` 的 `Clone` 是深拷贝（不是
                    // 引用计数），而走到这里 `read_buf` 必然为空——上面刚把它排干。
                    // 这是唯一的逐字节路径，白烧两次整包 memcpy。
                    shared.read_buf = msg.data;
                }
                Poll::Ready(Some(DataChannelEvent::OnClose)) | Poll::Ready(None) => {
                    shared.reading = None;
                    shared.eof = true;
                }
                // OnOpen / OnBufferedAmountLow 等与字节流无关，取下一个事件继续。
                Poll::Ready(Some(_)) => shared.reading = None,
                // 暂时没有更多消息：已搬到字节就先交出去。**只在一个字节都没有时才挂起**
                // ——`Ok(0)` 会被 `AsyncRead` 的调用方读成 EOF。
                Poll::Pending => {
                    return if filled > 0 {
                        Poll::Ready(Ok(filled))
                    } else {
                        Poll::Pending
                    };
                }
            }
        }
    }
}

impl AsyncWrite for PollDataChannel {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let dc = self.dc.clone();
        let mut shared = self.lock();
        // 上一条还没送出就先背压——同时也保证 SCTP 上的消息顺序。
        ready!(poll_pending_write(&mut shared, cx))?;

        let data = BytesMut::from(buf);
        shared.writing = Some(async move { dc.send(data).await }.boxed());
        // 立即驱动一次：多数情况能直接完成，省一轮唤醒。
        let _ = poll_pending_write(&mut shared, cx)?;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut shared = self.lock();
        poll_pending_write(&mut shared, cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let dc = self.dc.clone();
        let mut shared = self.lock();
        ready!(poll_pending_write(&mut shared, cx))?;

        if shared.closing.is_none() {
            shared.closing = Some(async move { dc.close().await }.boxed());
        }
        let fut = shared.closing.as_mut().expect("刚置入");
        let result = ready!(fut.poll_unpin(cx));
        shared.closing = None;
        Poll::Ready(result.map_err(to_io))
    }
}

fn to_io(e: webrtc::error::Error) -> io::Error {
    io::Error::other(e.to_string())
}

/// 等一条 DataChannel 真正进入 `Open`。
///
/// **必须在写第一个字节之前等。** `PeerConnection` 的 `connected` 只表示 DTLS 握手
/// 完成，而 DataChannel 要等 SCTP 关联建立才可用——实测两者差约 2 ms，期间
/// `dc.send()` 会**成功返回但把数据丢掉**（0.20 不为未 open 的通道排队）。
///
/// 症状极隐蔽：第一条消息静默消失，两端各自等对方，直到超时。这与 wasm 侧那条
/// 「`Connecting` 不是错误」是同一个时序问题的两面。
pub(crate) async fn await_open(dc: &Arc<dyn DataChannel>) -> Result<(), io::Error> {
    // 已经开了就别去 poll——那会白白吃掉一个事件。
    if matches!(dc.ready_state().await, Ok(RTCDataChannelState::Open)) {
        return Ok(());
    }

    loop {
        match dc.poll().await {
            Some(DataChannelEvent::OnOpen) => return Ok(()),
            Some(DataChannelEvent::OnClose) | None => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "DataChannel 在打开前就关闭了",
                ));
            }
            // OnError 未必致命（通道可能仍可用），继续等 OnOpen 或 OnClose。
            Some(_) => continue,
        }
    }
}
