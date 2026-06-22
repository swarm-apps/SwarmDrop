//! MCP Server 启停命令

use tauri::AppHandle;

use crate::mcp::server::{McpServerState, McpStatus};

/// 查询 MCP Server 当前状态
#[tauri::command]
#[specta::specta]
pub async fn get_mcp_status(
    state: tauri::State<'_, McpServerState>,
) -> crate::AppResult<McpStatus> {
    let guard = state.lock().await;
    match guard.as_ref() {
        Some(handle) => Ok(McpStatus {
            running: true,
            addr: Some(handle.addr.to_string()),
        }),
        None => Ok(McpStatus {
            running: false,
            addr: None,
        }),
    }
}

/// 启动 MCP Server
///
/// `port` 为可选参数，默认 19527。
#[tauri::command]
#[specta::specta]
pub async fn start_mcp_server(
    app: AppHandle,
    state: tauri::State<'_, McpServerState>,
    port: Option<u16>,
) -> crate::AppResult<McpStatus> {
    let mut guard = state.lock().await;

    // 已经在运行，直接返回当前状态
    if let Some(handle) = guard.as_ref() {
        return Ok(McpStatus {
            running: true,
            addr: Some(handle.addr.to_string()),
        });
    }

    let port = port.unwrap_or(19527);
    let handle = crate::mcp::server::start(app, port).await?;
    let addr = handle.addr.to_string();
    *guard = Some(handle);

    Ok(McpStatus {
        running: true,
        addr: Some(addr),
    })
}

/// 停止 MCP Server
#[tauri::command]
#[specta::specta]
pub async fn stop_mcp_server(
    state: tauri::State<'_, McpServerState>,
) -> crate::AppResult<McpStatus> {
    let mut guard = state.lock().await;

    if let Some(handle) = guard.take() {
        crate::mcp::server::stop(handle);
    }

    Ok(McpStatus {
        running: false,
        addr: None,
    })
}
