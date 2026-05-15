//! MCP Server 模块
//!
//! 基于 rmcp SDK 和 axum HTTP 框架实现嵌入式 MCP Server，
//! 让 AI 助手通过标准 MCP 协议操控 SwarmDrop 的 P2P 文件传输能力。

pub mod resources;
pub mod server;
mod tools;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{
    Implementation, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResult, ServerCapabilities, ServerInfo,
};
use rmcp::{tool_handler, ErrorData, RoleServer, ServerHandler};
use tauri::AppHandle;

/// MCP Handler：持有 AppHandle，通过 Tauri 状态树访问所有后端能力
#[derive(Clone)]
pub struct McpHandler {
    pub(crate) app: AppHandle,
    tool_router: ToolRouter<Self>,
}

#[tool_handler]
impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("swarmdrop", env!("CARGO_PKG_VERSION"))
                .with_title("SwarmDrop MCP Server"),
        )
        .with_instructions(
            "SwarmDrop P2P 文件传输 MCP 服务。\
             请先调用 get_network_status 确认节点已启动，\
             再调用 list_available_devices 查看可用设备，\
             最后调用 send_files 发送文件。",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(resources::list())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        resources::read(request)
    }
}

impl McpHandler {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            tool_router: Self::tool_router(),
        }
    }
}
