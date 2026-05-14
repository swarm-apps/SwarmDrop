//! MCP HTTP Server 启停管理

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tauri::AppHandle;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::McpHandler;
use crate::AppResult;

/// MCP Server 运行句柄
pub struct McpServerHandle {
    /// 用于触发 graceful shutdown
    cancel_token: CancellationToken,
    /// 实际监听地址
    pub addr: SocketAddr,
}

/// Tauri 托管的 MCP Server 状态
pub type McpServerState = Mutex<Option<McpServerHandle>>;

/// MCP Server 状态返回值
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub running: bool,
    pub addr: Option<String>,
}

/// 启动 MCP HTTP Server
pub async fn start(app: AppHandle, port: u16) -> AppResult<McpServerHandle> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    let cancel_token = CancellationToken::new();

    let mut config = StreamableHttpServerConfig::default();
    config.sse_keep_alive = Some(Duration::from_secs(15));
    config.stateful_mode = true;
    config.cancellation_token = cancel_token.clone();

    let session_manager = Arc::new(LocalSessionManager::default());

    // 每个 MCP 会话创建一个新的 McpHandler，共享同一个 AppHandle
    let app_clone = app.clone();
    let service = StreamableHttpService::new(
        move || Ok(McpHandler::new(app_clone.clone())),
        session_manager,
        config,
    );

    let router = Router::new().fallback_service(service);

    let token = cancel_token.clone();
    tokio::spawn(async move {
        info!("MCP Server 启动: {}", local_addr);
        axum::serve(listener, router)
            .with_graceful_shutdown(token.cancelled_owned())
            .await
            .ok();
        info!("MCP Server 已停止");
    });

    Ok(McpServerHandle {
        cancel_token,
        addr: local_addr,
    })
}

/// 停止 MCP Server
pub fn stop(handle: McpServerHandle) {
    handle.cancel_token.cancel();
}
