//! 工具的 schema 与分派。
//!
//! ⚠️ **本文件只依赖 [`super::backend`]，不得引用 `super::host`**。那条边一旦连上，
//! 工具面就绑死在这个宿主上——而它存在的理由正是让它别绑死（见 `backend` 的模块文档）。
//! 持有的是 `Arc<dyn ToolBackend>`，泛型也不要：宏展开与泛型参数一起用会把签名弄得
//! 很难读，而这里的动态分发发生在一次 RPC 的粒度上，代价可以忽略。
//!
//! ## 工具名与桌面端对齐
//!
//! 同名工具的语义必须与 `src-tauri/src/mcp/tools.rs` 一致（spec: `cli-mcp-host` 的
//! 「工具面」）。同一个 agent 会在两种宿主下遇到它们，名字一样而行为不同是最难排查的
//! 一类差异——模型不会报错，它只会做出对另一个宿主才正确的事。
//!
//! 逐条核对（2026-08-21）的结论：13 个同名工具语义一致，另有三处**有意**的差异——
//! 记在这里是为了让它们不被下一个人当成疏漏「修正」掉。
//!
//! | 差异 | 桌面端 | 这里 | 为什么 |
//! |---|---|---|---|
//! | `send_files` 的目标 | `peer_id`，要求精确、明说不许按名字猜 | `to`，名称或 `peerId` 都行 | 那条限制是为了防同名设备发错，而 CLI 的 `resolve_target` 在同名时**报错**不猜（`devices::target_error`）——安全性由解析器保证，不必收窄参数。CLI 用户本来也用名称 |
//! | 设备返回的字段 | `McpDevice`（含 `alias` / `groups` / `identityHint`） | `DeviceRow`（含 `online` / `pairedAt`） | 设备分组是桌面端独有的功能，CLI 没有那份数据；反过来 `online` 的三态（在线 / 离线 / **未知**）是 CLI 特有的诚实表达 |
//! | `send_text` | 没有 | 有 | CLI 独有的能力，不是命名分叉 |
//!
//! 桌面端 20 个工具里，这里有 13 个同名的，另加独有的 `send_text`。**没有**的 7 个：
//!
//! - `accept_transfer` / `reject_transfer` —— 见下方那条测试的理由（授权位）。
//! - `ensure_node_running` —— 这里的节点由 `swarmdrop mcp` 自己持有到退出，
//!   没有「先确保它起来」这一步（见 [`crate::cmd::mcp`]）。
//! - `get_receiving_paused` / `set_receiving_paused` / `archive_inbox_item` /
//!   `export_inbox_item` —— 超出 spec `cli-mcp-host` 约定的四类工具面。要加先改 spec。

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::Value;

use super::backend::{ToolBackend, ToolError};
use crate::runtime::transfers::Control;

#[derive(Clone)]
pub struct ToolHost {
    backend: Arc<dyn ToolBackend>,
    // rmcp 的 `#[tool_handler]` 宏按约定读取该 router，rustc 的 dead_code 分析看不到
    // 宏展开里的用途。
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl ToolHost {
    pub fn new(backend: Arc<dyn ToolBackend>) -> Self {
        Self {
            backend,
            tool_router: Self::tool_router(),
        }
    }
}

/// 成功：把负载原样交给模型。
///
/// `to_string_pretty` 而非紧凑形式：这段文字是模型上下文的一部分，缩进能让它在长清单里
/// 少认错字段。多出来的空白相对于字段名本身的开销可以忽略。
fn ok(value: &Value) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![
        rmcp::model::ContentBlock::text(
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        ),
    ]))
}

/// 失败：一律走 `CallToolResult::error` 而不是协议级的 [`ErrorData`]。
///
/// 两者对模型是两回事：`isError` 的结果**回到模型手里**，它可以据此改参数重试；
/// 协议级错误是「这次调用根本没成立」，宿主可能直接中断整轮。工具的业务失败属于前者。
fn failed(err: ToolError) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![
        rmcp::model::ContentBlock::text(err.to_string()),
    ]))
}

fn render(result: Result<Value, ToolError>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(value) => ok(&value),
        Err(err) => failed(err),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendFilesParams {
    /// 目标设备：`list_available_devices` 返回的 `name` 或 `peerId`。同名设备存在时用
    /// `peerId`，否则无法确定是哪一台。
    pub to: String,
    /// 要发送的文件或目录的绝对路径。
    pub file_paths: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendTextParams {
    /// 目标设备：`list_available_devices` 返回的 `name` 或 `peerId`。
    pub to: String,
    /// 正文（UTF-8，上限 64 KiB）。对端在收件箱里收到它，不是文件。
    pub body: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchInboxParams {
    /// 关键词。大小写不敏感，覆盖标题、来源设备名、文件名与文本正文。
    pub query: String,
    /// 可选：返回条数上限。缺省由内核决定，且不允许超过它的上限。
    pub limit: Option<u32>,
    /// 可选：是否纳入已归档条目，默认 false。
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListInboxParams {
    /// 可选：返回条数上限。
    pub limit: Option<u32>,
    /// 可选：是否纳入已归档条目，默认 false。
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InboxItemParams {
    /// 条目标识（完整 UUID，来自 `list_inbox` 或 `search_inbox`）。
    pub item_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InboxFileParams {
    /// 条目标识（完整 UUID）。
    pub item_id: String,
    /// 条目内文件的相对路径，取自 `search_inbox` / `get_inbox_item` 里的 `relativePath`。
    pub relative_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTransfersParams {
    /// 可选：返回条数上限，按更新时间倒序。
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TransferParams {
    /// 传输会话标识（完整 UUID，来自 `list_transfers`）。
    pub session_id: String,
}

#[tool_router]
impl ToolHost {
    #[tool(
        description = "获取本机 SwarmDrop 节点的运行状态：节点标识、已连接对端数、NAT 类型、中继可达性。发不出去时先看它。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_network_status(&self) -> Result<CallToolResult, ErrorData> {
        render(self.backend.network_status().await)
    }

    #[tool(
        description = "列出当前在线、可以立刻发送的已配对设备。name 是面向用户的设备名，peerId 是节点标识；同名设备存在时后续调用要用 peerId。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_available_devices(&self) -> Result<CallToolResult, ErrorData> {
        render(self.backend.list_devices(true).await)
    }

    #[tool(
        description = "列出全部已配对设备（含离线）。与 list_available_devices 互补：用它解释某台设备为什么发不出去。online 为 null 表示本机节点未运行、无从探测，不是「离线」。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_paired_devices(&self) -> Result<CallToolResult, ErrorData> {
        render(self.backend.list_devices(false).await)
    }

    #[tool(
        description = "向一台已配对设备发送文件或目录，阻塞到传输结束。对端可能需要人工确认。",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn send_files(
        &self,
        Parameters(params): Parameters<SendFilesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let paths: Vec<PathBuf> = params.file_paths.into_iter().map(PathBuf::from).collect();
        if paths.is_empty() {
            return failed(ToolError::invalid("file_paths 不能为空"));
        }
        render(self.backend.send_files(paths, &params.to).await)
    }

    #[tool(
        description = "向一台已配对设备发送一段文本，对端在收件箱里收到它。适合发链接、验证码、一段说明——不必先落成文件。",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn send_text(
        &self,
        Parameters(params): Parameters<SendTextParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(self.backend.send_text(params.body, &params.to).await)
    }

    #[tool(
        description = "按关键词检索本机收件箱，返回命中条目及其片段。先用它定位条目，再用 get_inbox_file 取文件的本地路径。只覆盖本机收件箱，不跨设备。",
        annotations(read_only_hint = true)
    )]
    pub async fn search_inbox(
        &self,
        Parameters(params): Parameters<SearchInboxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(
            self.backend
                .search_inbox(
                    &params.query,
                    params.limit,
                    params.include_archived.unwrap_or(false),
                )
                .await,
        )
    }

    #[tool(
        description = "按接收时间倒序列出收件箱条目，与 search_inbox 互补（不需要关键词时用它）。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_inbox(
        &self,
        Parameters(params): Parameters<ListInboxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(
            self.backend
                .list_inbox(params.limit, params.include_archived.unwrap_or(false))
                .await,
        )
    }

    #[tool(
        description = "取一个收件箱条目的完整详情：标题、来源设备、接收时间、文件清单（含 relativePath 与本地路径）。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_inbox_item(
        &self,
        Parameters(params): Parameters<InboxItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(self.backend.inbox_item(&params.item_id).await)
    }

    #[tool(
        description = "取收件箱条目内单个文件在本机的真实路径，用于把收到的文件交给别的工具处理。文件已被移走或删除时明确报错，不会返回无效路径。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_inbox_file(
        &self,
        Parameters(params): Parameters<InboxFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .backend
            .inbox_file_path(&params.item_id, &params.relative_path)
            .await
        {
            Ok(path) => ok(&serde_json::json!({ "localPath": path.to_string_lossy() })),
            Err(err) => failed(err),
        }
    }

    #[tool(
        description = "列出进行中与最近的传输会话：sessionId、方向、对端、阶段、进度、文件数。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_transfers(
        &self,
        Parameters(params): Parameters<ListTransfersParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(self.backend.list_transfers(params.limit).await)
    }

    #[tool(
        description = "按 sessionId 查询单条传输会话的详情：阶段、整体进度、分文件状态。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_transfer_status(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(self.backend.transfer_status(&params.session_id).await)
    }

    #[tool(
        description = "暂停一条进行中的传输。只对正在传输的会话成立——尚未开始或已结束的会话请用 cancel_transfer。",
        annotations(read_only_hint = false)
    )]
    pub async fn pause_transfer(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(
            self.backend
                .control_transfer(&params.session_id, Control::Pause)
                .await,
        )
    }

    #[tool(
        description = "恢复一条已暂停的传输。只对断点信息完好的会话成立；不可恢复的中断只能重新发一次。",
        annotations(read_only_hint = false)
    )]
    pub async fn resume_transfer(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(
            self.backend
                .control_transfer(&params.session_id, Control::Resume)
                .await,
        )
    }

    #[tool(
        description = "取消一条尚未结束的传输，并通知对端。",
        annotations(read_only_hint = false)
    )]
    pub async fn cancel_transfer(
        &self,
        Parameters(params): Parameters<TransferParams>,
    ) -> Result<CallToolResult, ErrorData> {
        render(
            self.backend
                .control_transfer(&params.session_id, Control::Cancel)
                .await,
        )
    }
}

#[tool_handler]
impl ServerHandler for ToolHost {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("swarmdrop", env!("CARGO_PKG_VERSION"))
                    .with_title("SwarmDrop CLI"),
            )
            .with_instructions(
                "SwarmDrop：无账号、无公网 IP 的设备间端到端加密传输。本 server 操作的是\
                 运行它的这台机器。\n\n\
                 发送：先 list_available_devices 看有哪些设备在线，再 send_files 发文件、\
                 send_text 发一段文字。目标用 name，同名设备存在时改用 peerId。\
                 对端可能需要人工确认，send_files 会一直等到有确定结果。\n\n\
                 取用户从别的设备发来的东西：search_inbox 按关键词找、list_inbox 按时间列，\
                 再用 get_inbox_file 取某个文件的本地路径交给别的工具。\n\n\
                 排查发不出去：get_network_status 看本机节点是否在跑、中继通不通；\
                 list_paired_devices 看目标设备是否离线（online 为 null 表示节点没跑，\
                 无从探测）。\n\n\
                 接收是节点在线时的被动行为，没有对应的工具——收不收由设备的主人决定，\
                 不由模型决定。",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_names() -> Vec<String> {
        ToolHost::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// 工具集合是**契约**，不是实现细节：同一个 agent 会在桌面端与 CLI 两种宿主下遇到
    /// 同名工具，增删都要先改 spec `cli-mcp-host` 的「工具面」。
    ///
    /// 这条测试的作用是让「顺手加一个」变成一次显式的决定。
    #[test]
    fn the_tool_surface_is_the_agreed_one() {
        let mut names = tool_names();
        names.sort();

        let mut expected = vec![
            // 发送
            "send_files",
            "send_text",
            // 设备
            "list_available_devices",
            "list_paired_devices",
            // 收件箱
            "list_inbox",
            "search_inbox",
            "get_inbox_item",
            "get_inbox_file",
            // 传输
            "list_transfers",
            "get_transfer_status",
            "pause_transfer",
            "resume_transfer",
            "cancel_transfer",
            // 节点
            "get_network_status",
        ];
        expected.sort();

        assert_eq!(names, expected);
    }

    /// **不得有「代收入站传输」类工具**（spec `cli-mcp-host` 里是一条 SHALL NOT）。
    ///
    /// CLI 的常驻节点对来自已配对设备的入站内容本就自行确认，模型不需要这项能力；
    /// 把它暴露出去等于把「收不收」这个决定从人手里移交给模型——而那台机器的主人
    /// 可能正不在电脑前。
    ///
    /// 桌面端有 `accept_transfer` / `reject_transfer`，但那两个背后有一道逐设备的
    /// 「允许 MCP 代收」授权位在守着；CLI 没有那道闸，所以不能照搬。要加之前，
    /// 先把授权位补上并改 spec。
    #[test]
    fn no_tool_decides_whether_to_accept_an_inbound_transfer() {
        for name in tool_names() {
            assert!(
                !name.contains("accept") && !name.contains("reject"),
                "{name} 让模型替人决定收不收——见本测试的文档注释"
            );
        }
    }
}
