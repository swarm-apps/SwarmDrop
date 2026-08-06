//! `PeerConnection` 的关闭守卫。
//!
//! # 为什么需要它
//!
//! `webrtc-rs` 的 `PeerConnection` 背后是一个**独立的 driver 任务**（`PeerConnectionDriver`
//! 的 event loop），它只在 `close()` 之后退出。上游的 `Drop` 只在 `dedicated_reactor`
//! 模式下设 shutdown 标志，并把 general-runtime（我们用的那条）判为
//! 「detach harmlessly onto the application's own worker pool」。
//!
//! **对已死的连接不成立。** 实测被 detach 的 driver 并不 park，而是在 `poll_timeout`
//! 上空转：2026-08-06 采样到桌面端 CPU **948%**，热点全在 `PeerConnectionDriver::event_loop`
//! / `RTCPeerConnection::poll_write` / `mach_absolute_time`，整个应用连同 webview 一起
//! 失去响应，看起来就是「卡死」。
//!
//! 泄漏有两条来源，缺一条都堵不住：
//!
//! 1. **握手失败**——`direct::upgrade` 的 `inbound`/`outbound` 在建成连接前有八个 `?`
//!    早退点，拨到一个不是 webrtc-direct 的端口、certhash 对不上、Noise 认证失败都会
//!    走到那里。而拨号是**会重试的**，于是泄漏按重试次数累积。
//! 2. **连接异常终止**——`StreamMuxer::poll_close` 只在 libp2p 走正常关闭流程时才被
//!    调到；对端掉线或 Swarm 直接丢弃连接时，muxer 是**直接被 drop** 的。
//!
//! 两处都用本类型持有连接，不变式就只剩一条：**只要 `PeerConnection` 被
//! [`ManagedPeerConnection`] 持有，它就一定会被关闭**。握手成功时把这个值本身
//! **move 给下一任持有者**（muxer），守卫就跟着连接走，中途一刻也没有裸 `Arc`。
//!
//! wasm 侧的对称实现在 [`backend::wasm::muxer`](crate::backend::wasm::muxer) 的 `Drop`
//! 里——那边泄漏的是浏览器自己管的对象，这边泄漏的是本进程的 CPU。

use std::ops::Deref;
use std::sync::Arc;

use webrtc::peer_connection::PeerConnection;
use webrtc::runtime::Runtime;

/// 持有一条 `PeerConnection`，drop 时保证它被关闭。
///
/// `Deref` 到 `Arc<dyn PeerConnection>`，所以调用点写法与裸 `Arc` 无异。
///
/// **刻意没有「取出内层」的口子。** 移交所有权就是把整个值 move 过去；一旦提供
/// `into_inner`，字段就得是 `Option`、`Deref` 就得 `expect`，而「已经交出去了还在用」
/// 这个状态本身是编译器本可以替我们排除掉的。
pub(crate) struct ManagedPeerConnection {
    pc: Arc<dyn PeerConnection>,
    runtime: Arc<dyn Runtime>,
}

impl ManagedPeerConnection {
    pub(crate) fn new(pc: Arc<dyn PeerConnection>, runtime: Arc<dyn Runtime>) -> Self {
        Self { pc, runtime }
    }
}

impl Deref for ManagedPeerConnection {
    type Target = Arc<dyn PeerConnection>;

    fn deref(&self) -> &Self::Target {
        &self.pc
    }
}

impl Drop for ManagedPeerConnection {
    fn drop(&mut self) {
        // `close()` 是 async 而 `Drop` 不能 await，只能派出去。重复 close 幂等，
        // 已经走过 `poll_close` 的连接再关一次无害。
        let pc = self.pc.clone();
        self.runtime.spawn(Box::pin(async move {
            if let Err(e) = pc.close().await {
                tracing::debug!("关闭 PeerConnection 失败：{e}");
            }
        }));
    }
}
