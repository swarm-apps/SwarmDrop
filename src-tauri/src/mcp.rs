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
use rmcp::{ErrorData, RoleServer, ServerHandler, tool_handler};
use tauri::AppHandle;

/// MCP Handler：持有 AppHandle，通过 Tauri 状态树访问所有后端能力
#[derive(Clone)]
pub struct McpHandler {
    pub(crate) app: AppHandle,
    // rmcp 的 #[tool_handler] 宏按约定读取该 router，rustc dead_code 分析看不到宏展开用途。
    #[allow(dead_code)]
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
            "SwarmDrop P2P 文件传输 MCP 服务（端到端加密、无服务器，仅监听本地）。\n\n\
             节点与接收控制：ensure_node_running 确保本机节点已上线（幂等，需身份在 app 内已解锁）——\
             很多操作的前置；get_receiving_paused / set_receiving_paused 查询 / 开关全局暂停接收。\n\n\
             发送文件：先 get_network_status 确认节点已启动，再 list_available_devices \
             查看在线的已配对设备，最后 send_files 发送（对方需在 SwarmDrop 中接受）。\
             list_paired_devices 可只读列出全部已配对设备及策略标志，解释某设备为何发不出 / 收不了。\n\n\
             跟踪与控制传输：list_transfers 列出进行中/最近会话，get_transfer_status 查单个会话，\
             cancel_transfer / pause_transfer / resume_transfer 控制传输。\n\n\
             代收入站文件：list_transfers 里 direction=receive、phase=Offered 的会话是等待确认的入站 offer，\
             用 accept_transfer（可带 save_path）接受、reject_transfer 拒绝。需来源设备在 SwarmDrop 设备策略中\
             开启「允许 MCP 代收」。挂起 offer 决策窗口约 5 分钟，要可靠代收需保持活跃轮询、别等被动唤醒。\n\n\
             查找已接收文件：search_inbox 按关键词检索或 list_inbox 列出收件箱，\
             get_inbox_item 取条目详情，get_inbox_file 取单个文件本地路径；archive_inbox_item 归档、\
             export_inbox_item 导出到指定目录。检索仅覆盖本机 inbox，不跨设备。",
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
