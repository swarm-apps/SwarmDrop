//! Typed RPC：裸流上的「一流一问一答」（旧栈 request-response behaviour 的替代）。
//!
//! 形态：open stream → 写请求帧 → 读响应帧 → 关流。
//! - yamux/QUIC 开流廉价，免请求关联 ID、免队头阻塞；
//! - **handler 可以在回复前 await 用户决策**（配对确认等长交互）——旧栈的
//!   pending_id / PendingMap / send_response 三件套因此整体消失；
//! - 业务层面的「拒绝/失败」编码进 `Resp` 类型本身（如 `OfferResult.accepted`）；
//!   handler 返回 `Err` 则直接断流，调用方看到 [`RpcError::ClosedEarly`]。
//!
//! 帧格式：`u32 BE 长度前缀 + CBOR`，控制面上限 1 MiB（数据面协议自带
//! 帧规则，不走本模块）。

use std::marker::PhantomData;
use std::time::Duration;

use futures::{AsyncReadExt, AsyncWriteExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use swarmdrop_net_base::{NodeId, ProtocolId};

use crate::endpoint::Endpoint;
use crate::error::{AcceptError, RpcError};
use crate::router::ProtocolHandler;
use crate::stream::P2pStream;

/// 控制面单帧上限。
pub const MAX_RPC_FRAME: usize = 1024 * 1024;

/// RPC 消息约束（CBOR 编解码 + 跨任务传递）。
pub trait RpcMessage: Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static {}
impl<T> RpcMessage for T where T: Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static {}

/// 调用选项。
#[derive(Debug, Clone, Copy)]
pub struct CallOptions {
    /// 整体超时（open + 写请求 + 等响应）。默认 120s——对齐旧栈
    /// `req_resp_timeout`，容纳「对端等用户决策」的长交互。
    pub timeout: Duration,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
        }
    }
}

/// 一个 typed RPC 协议（`Req` → `Resp`）。
///
/// ```ignore
/// const PAIRING: Rpc<PairingRequest, PairingResponse> =
///     Rpc::new(ProtocolId::from_static("/swarmdrop/pairing/2"));
/// // 客户端
/// let resp = PAIRING.call(&endpoint, peer, &request).await?;
/// // 服务端
/// Router::builder(endpoint).accept(PAIRING.protocol(), PAIRING.handler(service)).spawn();
/// ```
pub struct Rpc<Req, Resp> {
    protocol: ProtocolId,
    _marker: PhantomData<fn() -> (Req, Resp)>,
}

impl<Req, Resp> Clone for Rpc<Req, Resp> {
    fn clone(&self) -> Self {
        Self {
            protocol: self.protocol.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Req, Resp> std::fmt::Debug for Rpc<Req, Resp> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rpc({})", self.protocol)
    }
}

impl<Req: RpcMessage, Resp: RpcMessage> Rpc<Req, Resp> {
    /// 定义一个 RPC 协议（const 上下文可用）。
    pub const fn new(protocol: ProtocolId) -> Self {
        Self {
            protocol,
            _marker: PhantomData,
        }
    }

    /// 协议名。
    pub fn protocol(&self) -> ProtocolId {
        self.protocol.clone()
    }

    /// 发起调用（默认 120s 超时；未连接时 `open` 内部按需拨号）。
    pub async fn call(&self, endpoint: &Endpoint, to: NodeId, req: &Req) -> Result<Resp, RpcError> {
        self.call_with(endpoint, to, req, CallOptions::default())
            .await
    }

    /// 带选项调用。
    pub async fn call_with(
        &self,
        endpoint: &Endpoint,
        to: NodeId,
        req: &Req,
        options: CallOptions,
    ) -> Result<Resp, RpcError> {
        n0_future::time::timeout(options.timeout, async {
            let mut stream = endpoint.open(to, self.protocol.clone()).await?;
            write_frame(&mut stream, req).await?;
            stream.flush().await?;
            let resp = read_frame::<Resp>(&mut stream).await?;
            let _ = stream.close().await;
            Ok(resp)
        })
        .await
        .map_err(|_| RpcError::Timeout)?
    }

    /// 把业务 service 包成 [`ProtocolHandler`]，交给 Router 注册。
    pub fn handler<S: RpcService<Req, Resp>>(&self, service: S) -> RpcHandler<Req, Resp, S> {
        RpcHandler {
            protocol: self.protocol.clone(),
            service,
            _marker: PhantomData,
        }
    }
}

/// RPC 服务端：处理一个请求、产出一个响应。
///
/// 可以在 `handle` 里长时间 await（等用户决策）——每个请求跑在独立任务上。
/// 返回 `Err` 表示协议级失败：流被断开，调用方看到 `ClosedEarly`；
/// 业务级的拒绝/失败应编码进 `Resp` 类型。
pub trait RpcService<Req, Resp>: Send + Sync + 'static {
    fn handle(
        &self,
        from: NodeId,
        req: Req,
    ) -> impl Future<Output = Result<Resp, AcceptError>> + Send;
}

/// [`RpcService`] → [`ProtocolHandler`] 适配器（由 [`Rpc::handler`] 构造）。
pub struct RpcHandler<Req, Resp, S> {
    protocol: ProtocolId,
    service: S,
    _marker: PhantomData<fn() -> (Req, Resp)>,
}

impl<Req, Resp, S> std::fmt::Debug for RpcHandler<Req, Resp, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RpcHandler({})", self.protocol)
    }
}

impl<Req, Resp, S> ProtocolHandler for RpcHandler<Req, Resp, S>
where
    Req: RpcMessage,
    Resp: RpcMessage + Sync,
    S: RpcService<Req, Resp>,
{
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let req = read_frame::<Req>(&mut stream)
            .await
            .map_err(rpc_to_accept)?;
        let resp = self.service.handle(stream.remote(), req).await?;
        write_frame(&mut stream, &resp)
            .await
            .map_err(rpc_to_accept)?;
        stream.flush().await?;
        let _ = stream.close().await;
        Ok(())
    }
}

fn rpc_to_accept(err: RpcError) -> AcceptError {
    match err {
        RpcError::Io(e) => AcceptError::Io(e),
        RpcError::Decode(e) => AcceptError::Decode(e),
        RpcError::ClosedEarly => AcceptError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "stream closed before frame",
        )),
        other => AcceptError::Io(std::io::Error::other(other.to_string())),
    }
}

/// 写一帧：`u32 BE 长度 + CBOR`。
async fn write_frame<T: Serialize>(
    io: &mut (impl futures::AsyncWrite + Unpin),
    value: &T,
) -> Result<(), RpcError> {
    let bytes =
        cbor4ii::serde::to_vec(Vec::new(), value).map_err(|e| RpcError::Encode(e.to_string()))?;
    if bytes.len() > MAX_RPC_FRAME {
        return Err(RpcError::FrameTooLarge(bytes.len()));
    }
    io.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    io.write_all(&bytes).await?;
    Ok(())
}

/// 读一帧。对端在帧边界前关流 → [`RpcError::ClosedEarly`]；超限 → `FrameTooLarge`（防 OOM）。
async fn read_frame<T: DeserializeOwned>(
    io: &mut (impl futures::AsyncRead + Unpin),
) -> Result<T, RpcError> {
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            RpcError::ClosedEarly
        } else {
            RpcError::Io(e)
        }
    })?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_RPC_FRAME {
        return Err(RpcError::FrameTooLarge(len));
    }
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    cbor4ii::serde::from_slice(&buf).map_err(|e| RpcError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Msg {
        text: String,
        n: u64,
    }

    #[tokio::test]
    async fn frame_roundtrip() {
        let msg = Msg {
            text: "你好".into(),
            n: 42,
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();
        let back: Msg = read_frame(&mut futures::io::Cursor::new(buf))
            .await
            .unwrap();
        assert_eq!(back, msg);
    }

    /// 防 OOM 的关键路径：恶意长度前缀必须在**分配前**被拒。
    #[tokio::test]
    async fn read_rejects_oversized_length_prefix_before_allocating() {
        // 声称 4GiB-1 的帧，实际没有任何数据
        let mut evil = Vec::new();
        evil.extend_from_slice(&u32::MAX.to_be_bytes());
        let err = read_frame::<Msg>(&mut futures::io::Cursor::new(evil))
            .await
            .unwrap_err();
        assert!(
            matches!(err, RpcError::FrameTooLarge(n) if n == u32::MAX as usize),
            "got: {err:?}"
        );
    }

    #[tokio::test]
    async fn write_rejects_oversized_payload() {
        let msg = Msg {
            text: "x".repeat(MAX_RPC_FRAME + 1),
            n: 0,
        };
        let mut buf = Vec::new();
        let err = write_frame(&mut buf, &msg).await.unwrap_err();
        assert!(matches!(err, RpcError::FrameTooLarge(_)), "got: {err:?}");
        assert!(buf.is_empty(), "超限帧不得写出任何字节");
    }

    /// 对端在帧边界前关流 → ClosedEarly（RPC 语义：handler 拒答）。
    #[tokio::test]
    async fn eof_before_frame_is_closed_early() {
        let err = read_frame::<Msg>(&mut futures::io::Cursor::new(Vec::new()))
            .await
            .unwrap_err();
        assert!(matches!(err, RpcError::ClosedEarly), "got: {err:?}");

        // 长度前缀读了一半也算 ClosedEarly
        let err = read_frame::<Msg>(&mut futures::io::Cursor::new(vec![0u8, 0]))
            .await
            .unwrap_err();
        assert!(matches!(err, RpcError::ClosedEarly), "got: {err:?}");
    }

    /// 长度合法但 CBOR 无法解码 → Decode（不是 panic）。
    #[tokio::test]
    async fn garbage_payload_is_decode_error() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let err = read_frame::<Msg>(&mut futures::io::Cursor::new(buf))
            .await
            .unwrap_err();
        assert!(matches!(err, RpcError::Decode(_)), "got: {err:?}");
    }
}
