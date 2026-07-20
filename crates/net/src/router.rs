//! [`Router`] + [`ProtocolHandler`]：入站流按协议路由。
//!
//! 与 iroh 的差异（刻意）：路由粒度是 **stream** 而非 connection——
//! libp2p 一条连接经 multistream-select 天然跑多协议子流，协商发生在
//! 流打开时；libp2p-stream 的 `IncomingStreams` 恰好按协议预分类。
//!
//! 「ergonomic trait + Dyn trait + blanket impl」（iroh protocol.rs 同款）：
//! 用户写 `async fn accept(&self, stream)`，内部存 `Box<dyn DynProtocolHandler>`。

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::SelectAll;
use libp2p::{PeerId, StreamProtocol};
use n0_future::task::JoinSet;
use swarmdrop_net_base::{NodeId, ProtocolId};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::endpoint::Endpoint;
use crate::error::AcceptError;
use crate::stream::{Direction, P2pStream};

/// handler 关停宽限：先并发调 `shutdown()`，超时后 abort 存活任务。
const HANDLER_SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// 协议处理器。实现 `accept`（每条入站流在独立任务上调用）即可注册进 Router。
///
/// - `&self` 而非 `&mut self`——内部状态自备 `Arc<Mutex<_>>`；
/// - 返回 `Err` 只会被记一行 warn 然后 drop 流，**不给对端发错误码**——
///   要让对端知道原因，在返回前自行写响应帧；
/// - 长连接协议在 `accept` 里循环即可（future 跑在独立任务上，可长期存活）。
pub trait ProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    /// 处理一条入站流。
    fn accept(&self, stream: P2pStream) -> impl Future<Output = Result<(), AcceptError>> + Send;

    /// 优雅关停钩子：`Router::shutdown` 会并发调用所有 handler 的本方法。
    fn shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// dyn trait 方法返回的 boxed future（借 `&self` 生命周期；
/// native = Send，wasm 单线程无此约束）。
#[cfg(not(wasm_browser))]
type BoxedFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(wasm_browser)]
type BoxedFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + 'a>>;

/// [`ProtocolHandler`] 的 dyn-safe 化（RPITIT 不是 object-safe）。
pub(crate) trait DynProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    fn accept(&self, stream: P2pStream) -> BoxedFuture<'_, Result<(), AcceptError>>;
    fn shutdown(&self) -> BoxedFuture<'_, ()>;
}

impl<T: ProtocolHandler> DynProtocolHandler for T {
    fn accept(&self, stream: P2pStream) -> BoxedFuture<'_, Result<(), AcceptError>> {
        Box::pin(ProtocolHandler::accept(self, stream))
    }

    fn shutdown(&self) -> BoxedFuture<'_, ()> {
        Box::pin(ProtocolHandler::shutdown(self))
    }
}

type HandlerMap = BTreeMap<ProtocolId, Box<dyn DynProtocolHandler>>;

/// Router 构建器。`accept()` 只是登记，`spawn()` 才注册协议并起 accept 循环。
#[derive(Debug)]
pub struct RouterBuilder {
    endpoint: Endpoint,
    handlers: HandlerMap,
}

/// 入站流路由器。持有 accept 循环任务；drop 即 abort，推荐显式 `shutdown()`。
#[derive(Debug)]
pub struct Router {
    endpoint: Endpoint,
    cancel: CancellationToken,
    task: std::sync::Mutex<Option<n0_future::task::JoinHandle<()>>>,
}

impl Router {
    /// 新建构建器。
    pub fn builder(endpoint: Endpoint) -> RouterBuilder {
        RouterBuilder {
            endpoint,
            handlers: BTreeMap::new(),
        }
    }

    /// 所路由的 Endpoint。
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// 优雅关停：停 accept 循环 → 并发调 handler shutdown → 宽限后 abort。
    ///
    /// 只关 Router，不关 Endpoint——完整关停顺序是
    /// `router.shutdown().await` 然后 `endpoint.close().await`。
    pub async fn shutdown(&self) {
        self.cancel.cancel();
        let task = self.task.lock().expect("lock").take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl RouterBuilder {
    /// 注册协议处理器。
    ///
    /// # Panics
    /// 同一 `ProtocolId` 注册两次会 panic——重复注册是编程错误，
    /// 静默覆盖会造成「其中一个协议神秘失灵」的不可观测故障（iroh 之坑）。
    pub fn accept(mut self, protocol: ProtocolId, handler: impl ProtocolHandler) -> Self {
        let prev = self.handlers.insert(protocol.clone(), Box::new(handler));
        assert!(
            prev.is_none(),
            "protocol {protocol} registered twice on the same Router"
        );
        self
    }

    /// 注册所有协议并启动 accept 循环。
    ///
    /// # Panics
    /// 协议已被其他 Router / Control 注册时 panic（同上，显式失败优于静默失灵）。
    pub fn spawn(self) -> Router {
        let Self { endpoint, handlers } = self;

        // 逐协议注册入站流源（此刻起对端才能开这些协议的流）
        let mut incoming = SelectAll::new();
        let mut control = endpoint.stream_control();
        for protocol in handlers.keys() {
            let stream_protocol = StreamProtocol::try_from_owned(protocol.as_str().to_owned())
                .expect("ProtocolId guarantees '/' prefix");
            let streams = control
                .accept(stream_protocol)
                .unwrap_or_else(|e| panic!("protocol {protocol} already registered: {e}"));
            let protocol = protocol.clone();
            incoming.push(
                streams
                    .map(move |(peer, stream)| (protocol.clone(), peer, stream))
                    .boxed(),
            );
        }

        let handlers = Arc::new(handlers);
        let cancel = CancellationToken::new();
        let task = n0_future::task::spawn(run_loop(
            incoming,
            handlers,
            endpoint.clone(),
            cancel.clone(),
        ));

        Router {
            endpoint,
            cancel,
            task: std::sync::Mutex::new(Some(task)),
        }
    }
}

type IncomingSelect =
    SelectAll<futures::stream::BoxStream<'static, (ProtocolId, PeerId, libp2p::Stream)>>;

async fn run_loop(
    mut incoming: IncomingSelect,
    handlers: Arc<HandlerMap>,
    endpoint: Endpoint,
    cancel: CancellationToken,
) {
    let registry = endpoint.registry();
    let mut tasks: JoinSet<()> = JoinSet::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            next = incoming.next() => {
                let Some((protocol, peer, stream)) = next else { break };
                // 配额检查：超限显式拒绝（drop 即 reset 流），不静默排队
                let Some(guard) = registry.try_acquire(peer, protocol.clone(), Direction::Inbound)
                else {
                    warn!(%protocol, %peer, "inbound stream rejected: limit exceeded");
                    drop(stream);
                    continue;
                };
                let p2p = P2pStream::new(
                    NodeId::from_peer_id(peer),
                    protocol.clone(),
                    Direction::Inbound,
                    stream,
                    Some(guard),
                );
                let handlers = handlers.clone();
                tasks.spawn(async move {
                    let handler = handlers
                        .get(p2p.protocol())
                        .expect("stream source only exists for registered protocols");
                    let (protocol, remote) = (p2p.protocol().clone(), p2p.remote());
                    if let Err(e) = handler.accept(p2p).await {
                        warn!(%protocol, %remote, error = %e, "protocol handler failed");
                    }
                });
            }
            // 回收已结束的 handler 任务（panic 在此显形，不掀翻循环）
            Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                if let Err(e) = result {
                    warn!(error = %e, "protocol handler task panicked");
                }
            }
        }
    }

    // 关停编排：并发调所有 handler 的 shutdown（宽限内），随后 abort 存活任务
    let shutdowns = handlers.values().map(|h| h.shutdown());
    if n0_future::time::timeout(HANDLER_SHUTDOWN_GRACE, futures::future::join_all(shutdowns))
        .await
        .is_err()
    {
        debug!("handler shutdown grace elapsed");
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}
