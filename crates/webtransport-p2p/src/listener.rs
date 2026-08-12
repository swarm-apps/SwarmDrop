//! 监听：把 `wtransport` 的 async accept 循环桥成 poll 风格的事件流。
//!
//! # 两条边界
//!
//! ```text
//! ┌─ 后台 accept task ──────────────┐   mpsc   ┌─ Transport::poll ─┐
//! │ endpoint.accept().await         │  ────▶   │ 取出 Connection    │
//! │ ├─ 每条连接再 spawn 一个子任务   │          │ 包成 upgrade future│
//! │ │  ├─ QUIC 握手                 │          │ 交给 Swarm 驱动     │
//! │ │  ├─ 校验 :path 与 ?type       │          └───────────────────┘
//! │ │  └─ SessionRequest::accept    │
//! └─────────────────────────────────┘
//! ```
//!
//! - **后台 task 只做「接受连接」，不碰任何 libp2p 语义** —— 它不认识 `PeerId`、不跑
//!   Noise。于是它的失败模式只有一种（endpoint 挂了），生命周期与 listener 一一对应。
//! - **Noise 握手不在这里跑**，而是包进 `TransportEvent::Incoming` 的 upgrade future 交给
//!   Swarm 去 poll。这自动满足「别把 transport 驱动和数据传输绑在同一个 task 上」——
//!   握手的驱动权在 Swarm，我们的 task 只管收连接。
//!
//! # 为什么每条连接要再 spawn 一次
//!
//! `IncomingSession.await` 是完整的 QUIC 握手，`SessionRequest::accept().await` 还要一个
//! RTT。放在 accept 循环里直接 await，**一个慢客户端（或故意不完成握手的攻击者）就能把
//! 整个监听端口堵死**。
//!
//! 但 spawn 本身也要有闸：见 [`MAX_INFLIGHT_HANDSHAKES`] 与 [`INBOUND_HANDSHAKE_TIMEOUT`]。
//! 「靠 mpsc 的容量做背压」是不成立的 —— 那个容量会随 Sender clone 数一起涨。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc;
use libp2p_core::Multiaddr;
use wtransport::endpoint::IncomingSession;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Connection, Endpoint, Identity, ServerConfig};

use crate::addr::{self, Certhash};
use crate::error::Error;

/// spec 规定的 HTTP endpoint 路径。
const WELL_KNOWN_PATH: &str = "/.well-known/libp2p-webtransport";

/// spec 规定的握手类型查询参数。
const NOISE_QUERY: &str = "type=noise";

/// 已完成握手、等待被 `Transport::poll` 取走的连接队列上限。
///
/// ⚠️ **`mpsc::channel(n)` 的真实容量是 `n + Sender 个数`** —— 每 clone 一个 Sender 就多一个
/// 保证槽位。所以光靠这个常量**得不到背压**：accept 循环给每条连接 clone 一个 Sender 的话，
/// 每条都有自己的免排队槽位，这个数字形同虚设（本文件第一版就是这么写的）。
/// 真正的上限由 [`MAX_INFLIGHT_HANDSHAKES`] 的许可数给。
const INCOMING_CAPACITY: usize = 32;

/// 同时在做 QUIC + CONNECT 握手的入站连接数上限。
///
/// **这是抗 DoS 的那道闸**：`incoming.await` 是完整的 QUIC 握手，任何人都能对监听端口
/// 批量发起，每条各占一个 task 与一份 quinn 连接状态。没有它的话，公网 relay 上这是一条
/// 可被远程触发的内存增长路径 —— 只受 quinn 的 idle timeout 约束。
///
/// 拿不到许可就直接丢弃那条 incoming（对端会超时重试），不排队 —— 排队本身就是被撑爆的
/// 那个东西。
const MAX_INFLIGHT_HANDSHAKES: usize = 64;

/// 单条入站连接从 QUIC 握手到 CONNECT 应答的上限。
///
/// 与 `Config::handshake_timeout` 是**两段不同的超时**：那个盖的是交付给 Swarm 之后的
/// Noise 握手，管不到这一段。少了它，一个只完成 UDP 往返就不再说话的客户端能一直占着许可。
const INBOUND_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// 一个 WebTransport 监听端口。
pub(crate) struct Listener {
    /// `Endpoint` 不是 `Clone`（它是端口的所有者），而 accept 循环要与本结构共享它，
    /// 故套一层 `Arc`。`reload_config` 收 `&self`，共享后仍能换证书。
    endpoint: Arc<Endpoint<Server>>,
    incoming: mpsc::Receiver<Connection>,
    accept_task: tokio::task::JoinHandle<()>,
    /// 实际绑定的 socket。端口 0 已在这里解析成真实端口。
    bound: SocketAddr,
    /// 当前通告出去的地址。轮换时要为它们逐条发 `AddressExpired`，
    /// 所以必须记住**发出去的那一份**，不能事后重算 —— 网卡列表可能已经变了。
    announced: Vec<Multiaddr>,
}

impl Listener {
    /// 绑定端口并起 accept 循环。
    pub(crate) fn bind(
        socket: SocketAddr,
        identity: Identity,
        certhashes: &[Certhash],
    ) -> Result<Self, Error> {
        let endpoint = Arc::new(Endpoint::server(server_config(socket, identity)).map_err(
            |source| Error::Bind {
                addr: socket,
                source,
            },
        )?);

        let bound = endpoint.local_addr().map_err(|source| Error::Bind {
            addr: socket,
            source,
        })?;

        let (tx, incoming) = mpsc::channel(INCOMING_CAPACITY);
        let accept_task = tokio::spawn(accept_loop(Arc::clone(&endpoint), tx));

        Ok(Self {
            announced: addr::announce_addrs(bound, certhashes),
            endpoint,
            incoming,
            accept_task,
            bound,
        })
    }

    /// 当前通告的地址。
    pub(crate) fn announced(&self) -> &[Multiaddr] {
        &self.announced
    }

    /// 实际绑定的 socket。入站连接的 `local_addr` 用它。
    pub(crate) fn bound(&self) -> SocketAddr {
        self.bound
    }

    /// 换掉服务端证书，并重算通告地址。
    ///
    /// `rebind = false`：**不重新绑定 socket** —— 既有连接因此不受影响，这正是选
    /// `wtransport` 的决定性理由（`web-transport-quinn` 没有对应 API）。
    ///
    /// 返回被替换掉的旧地址，调用方据此发 `AddressExpired`。
    pub(crate) fn reload(
        &mut self,
        identity: Identity,
        certhashes: &[Certhash],
    ) -> Result<Vec<Multiaddr>, Error> {
        self.endpoint
            .reload_config(server_config(self.bound, identity), false)
            .map_err(|source| Error::Bind {
                addr: self.bound,
                source,
            })?;

        let fresh = addr::announce_addrs(self.bound, certhashes);
        Ok(std::mem::replace(&mut self.announced, fresh))
    }

    /// 取一条已完成握手的入站连接。
    pub(crate) fn poll_incoming(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Connection>> {
        use futures::StreamExt;
        self.incoming.poll_next_unpin(cx)
    }
}

impl Drop for Listener {
    /// accept 循环持着 endpoint 的一份 handle，不 abort 它端口就不会释放。
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

fn server_config(socket: SocketAddr, identity: Identity) -> ServerConfig {
    ServerConfig::builder()
        .with_bind_address(socket)
        .with_identity(identity)
        .build()
}

/// accept 循环。每条入站连接交给一个独立子任务，见模块文档。
async fn accept_loop(endpoint: Arc<Endpoint<Server>>, tx: mpsc::Sender<Connection>) {
    let inflight = Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_HANDSHAKES));

    loop {
        let incoming = endpoint.accept().await;
        let remote = incoming.remote_address();

        // 拿不到许可就直接丢 —— 排队等于把要防的那个堆积换个地方发生。
        let Ok(permit) = Arc::clone(&inflight).try_acquire_owned() else {
            tracing::warn!(
                %remote,
                limit = MAX_INFLIGHT_HANDSHAKES,
                "在途 WebTransport 握手已达上限，丢弃入站连接"
            );
            continue;
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            // 许可活到本任务结束（含送进队列那一步），drop 时自动归还。
            let _permit = permit;

            match crate::error::with_timeout(handshake(incoming), INBOUND_HANDSHAKE_TIMEOUT).await {
                Ok(Some(connection)) => {
                    use futures::SinkExt;
                    let mut tx = tx;
                    // 送不进去只有一种原因：Transport 已经被 drop。静默返回，
                    // 连接随之关闭。
                    let _ = tx.send(connection).await;
                }
                // 路径不匹配已在 handshake 内部回了 404，这里无事可做。
                Ok(None) => {}
                Err(e) => tracing::debug!(%remote, error = ?e, "webtransport 入站握手失败"),
            }
        });
    }
}

/// 完成 QUIC + CONNECT 握手，并把不是 libp2p 的请求挡在外面。
///
/// 返回 `Ok(None)` 表示「已按 spec 以 404 拒绝」——它不是错误，任何人都可以往这个端口
/// 发普通的 HTTP/3 请求。
async fn handshake(incoming: IncomingSession) -> Result<Option<Connection>, Error> {
    let request = incoming
        .await
        .map_err(|e| Error::Connect(format!("QUIC 握手失败：{e}")))?;

    if !is_libp2p_noise_endpoint(request.path()) {
        tracing::debug!(
            path = request.path(),
            remote = %request.remote_address(),
            "非 libp2p WebTransport 请求，以 404 拒绝"
        );
        request.not_found().await;
        return Ok(None);
    }

    let connection = request
        .accept()
        .await
        .map_err(|e| Error::Connect(format!("接受 WebTransport 会话失败：{e}")))?;

    Ok(Some(connection))
}

/// `:path` 是否是 spec 规定的 libp2p WebTransport endpoint。
///
/// 判据两条，缺一不可：路径恰为 `/.well-known/libp2p-webtransport`，且查询串里含
/// `type=noise`。后者是 spec 为「将来可能有别的握手类型」留的判别位 —— 缺了它不能当
/// 默认 noise 处理，那会让一个声明了别的类型的客户端拿到一条它读不懂的 Noise 流。
fn is_libp2p_noise_endpoint(raw_path: &str) -> bool {
    let (path, query) = raw_path.split_once('?').unwrap_or((raw_path, ""));

    path == WELL_KNOWN_PATH && query.split('&').any(|param| param == NOISE_QUERY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_spec_endpoint() {
        assert!(is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?type=noise"
        ));
    }

    /// 多个查询参数、顺序不定都要能认出来 —— 客户端可以附加自己的参数。
    #[test]
    fn accepts_noise_among_other_query_params() {
        assert!(is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?foo=bar&type=noise"
        ));
        assert!(is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?type=noise&foo=bar"
        ));
    }

    /// 路径不对必须拒 —— 这个端口上会收到各种普通 HTTP/3 请求（探测、扫描）。
    #[test]
    fn rejects_other_paths() {
        assert!(!is_libp2p_noise_endpoint("/?type=noise"));
        assert!(!is_libp2p_noise_endpoint("/libp2p-webtransport?type=noise"));
        assert!(!is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport/extra?type=noise"
        ));
    }

    /// 缺 `type=noise` 必须拒，不能当默认值处理 —— 那会让声明了别的握手类型的客户端
    /// 拿到一条它读不懂的 Noise 流。
    #[test]
    fn rejects_missing_or_wrong_handshake_type() {
        assert!(!is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport"
        ));
        assert!(!is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?type=tls"
        ));
        // 前缀相同但不是同一个参数
        assert!(!is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?type=noisexyz"
        ));
        assert!(!is_libp2p_noise_endpoint(
            "/.well-known/libp2p-webtransport?xtype=noise"
        ));
    }
}
