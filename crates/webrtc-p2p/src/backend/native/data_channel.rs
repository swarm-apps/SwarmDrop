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
        loop {
            if !shared.read_buf.is_empty() {
                let n = buf.len().min(shared.read_buf.len());
                buf[..n].copy_from_slice(&shared.read_buf.split_to(n));
                return Poll::Ready(Ok(n));
            }
            if shared.eof {
                // 0 字节即 EOF，这是 AsyncRead 的约定。
                return Poll::Ready(Ok(0));
            }

            if shared.reading.is_none() {
                let dc = dc.clone();
                shared.reading = Some(async move { dc.poll().await }.boxed());
            }
            let fut = shared.reading.as_mut().expect("刚置入");
            match ready!(fut.poll_unpin(cx)) {
                Some(DataChannelEvent::OnMessage(msg)) => {
                    shared.reading = None;
                    let data = msg.data.clone();
                    shared.read_buf.extend_from_slice(&data);
                }
                Some(DataChannelEvent::OnClose) | None => {
                    shared.reading = None;
                    shared.eof = true;
                }
                // OnOpen / OnBufferedAmountLow 等与字节流无关，取下一个事件继续。
                Some(_) => shared.reading = None,
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
