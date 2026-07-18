//! swarmdrop-net 浏览器冒烟壳。
//!
//! 共享部分（[`proto`]）在两端完全同一份——「rust-wasm 单核心包」的直接证据：
//! 协议定义、RPC 类型、Endpoint API 零 cfg 跨端。

/// 两端共享的冒烟协议（native bin 与 wasm 壳同一份定义）。
pub mod proto {
    use serde::{Deserialize, Serialize};
    use swarmdrop_net::{ProtocolId, Rpc};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EchoReq {
        pub text: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EchoResp {
        pub text: String,
        /// 应答方视角看到的调用方身份（验证传输层身份穿透）。
        pub from: String,
    }

    pub const SMOKE_ECHO: Rpc<EchoReq, EchoResp> =
        Rpc::new(ProtocolId::from_static("/swarmdrop/smoke-echo/1"));

    /// 两端共用的 echo 服务。
    #[derive(Debug, Clone)]
    pub struct EchoService;

    impl swarmdrop_net::RpcService<EchoReq, EchoResp> for EchoService {
        async fn handle(
            &self,
            from: swarmdrop_net::NodeId,
            req: EchoReq,
        ) -> Result<EchoResp, swarmdrop_net::AcceptError> {
            tracing::info!(%from, text = %req.text, "echo request");
            Ok(EchoResp {
                text: req.text,
                from: from.to_string(),
            })
        }
    }
}

// ============ wasm 壳（浏览器）============
#[cfg(target_arch = "wasm32")]
mod wasm {
    use swarmdrop_net::{Endpoint, NodeAddr, Router, presets};
    use wasm_bindgen::prelude::*;

    use crate::proto::{EchoReq, EchoService, SMOKE_ECHO};

    #[wasm_bindgen(start)]
    fn start() {
        console_error_panic_hook::set_once();
        tracing_subscriber::fmt()
            // 浏览器无 std 时钟，不去掉会 runtime error
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(tracing_subscriber_wasm::MakeConsoleWriter::default())
            .init();
    }

    /// 浏览器端节点：Browser preset（ws-websys + webrtc-websys + relay client，
    /// 无本地 listen）+ 同一份 echo 协议挂 Router。
    #[wasm_bindgen]
    pub struct WebNode {
        endpoint: Endpoint,
        _router: Router,
    }

    #[wasm_bindgen]
    impl WebNode {
        /// 建节点并挂 echo 服务（circuit listen 就绪后对端可反向调用）。
        pub async fn spawn() -> Result<WebNode, JsError> {
            let endpoint = Endpoint::builder()
                .preset(presets::Browser)
                .identify_protocol("/swarmdrop/2.0.0")
                .agent_version("net-web-smoke/wasm")
                .bind()
                .await
                .map_err(err)?;
            let router = Router::builder(endpoint.clone())
                .accept(SMOKE_ECHO.protocol(), SMOKE_ECHO.handler(EchoService))
                .spawn();
            Ok(WebNode {
                endpoint,
                _router: router,
            })
        }

        /// 本节点身份（base58）。
        pub fn node_id(&self) -> String {
            self.endpoint.node_id().to_string()
        }

        /// 拨任意 multiaddr（`.../ws` 或 `.../webrtc-direct/certhash/...`，
        /// 须带 `/p2p/<id>` 尾段）。返回连接路径描述。
        pub async fn connect(&self, addr: String) -> Result<String, JsError> {
            let (id, addr) = split_p2p_addr(&addr)?;
            let info = self
                .endpoint
                .connect(NodeAddr::with_addrs(id, vec![addr]))
                .await
                .map_err(err)?;
            Ok(format!("connected: path={:?} addr={}", info.path, info.addr))
        }

        /// 经 helper 请求 circuit reservation（浏览器被动接收连接的唯一入口）。
        /// 成功后对端可拨 `<helper-addr>/p2p/<helper>/p2p-circuit/p2p/<本机>`。
        pub async fn reserve(&self, helper_addr: String) -> Result<String, JsError> {
            let (id, addr) = split_p2p_addr(&helper_addr)?;
            self.endpoint
                .ensure_relay_reservation(NodeAddr::with_addrs(id, vec![addr]))
                .await
                .map_err(err)?;
            // 等 reservation Active（watch 采样）
            let mut relays = self.endpoint.watch_relays();
            loop {
                if relays
                    .get()
                    .get(&id)
                    .is_some_and(|s| *s == swarmdrop_net::RelayState::Active)
                {
                    return Ok(format!(
                        "reservation active; circuit addr = {helper_addr}/p2p-circuit/p2p/{}",
                        self.endpoint.node_id()
                    ));
                }
                if relays.updated().await.is_none() {
                    return Err(JsError::new("endpoint closed while waiting"));
                }
            }
        }

        /// 对某节点发起 RPC echo。
        pub async fn echo(&self, to: String, text: String) -> Result<String, JsError> {
            let to = to.parse().map_err(err)?;
            let resp = SMOKE_ECHO
                .call(&self.endpoint, to, &EchoReq { text })
                .await
                .map_err(err)?;
            Ok(format!("echo ok: {:?} (remote saw us as {})", resp.text, resp.from))
        }

        pub async fn close(self) {
            self.endpoint.close().await;
        }
    }

    fn err(e: impl std::fmt::Display) -> JsError {
        JsError::new(&e.to_string())
    }

    /// 把 `<addr>/p2p/<id>` 拆成 (NodeId, Addr)。
    fn split_p2p_addr(s: &str) -> Result<(swarmdrop_net::NodeId, swarmdrop_net::Addr), JsError> {
        let s = s.trim();
        let (addr, id) = s
            .rsplit_once("/p2p/")
            .ok_or_else(|| JsError::new("地址须以 /p2p/<node-id> 结尾"))?;
        Ok((id.parse().map_err(err)?, addr.parse().map_err(err)?))
    }
}
