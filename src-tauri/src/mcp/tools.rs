//! MCP Tool 实现
//!
//! 提供 12 个 Tool：
//! - 网络/发送：get_network_status、list_available_devices、send_files
//! - 传输生命周期：list_transfers、get_transfer_status、cancel_transfer、pause_transfer、resume_transfer
//! - 收件箱：search_inbox、list_inbox、get_inbox_item、get_inbox_file

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, schemars, tool, tool_router};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use swarmdrop_core::transfer::manager::TransferManager;
use tauri::Manager;
use uuid::Uuid;

use super::McpHandler;
use crate::device::{DeviceFilter, DeviceStatus};
use crate::host::file_source::{EnumeratedFile, FileSource};
use crate::network::NetManagerState;

/// 辅助：构造 MCP 错误结果（isError: true）
fn mcp_error(msg: impl std::fmt::Display) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![
        rmcp::model::ContentBlock::text(msg.to_string()),
    ]))
}

/// 辅助：构造 MCP 成功结果
fn mcp_ok(json: String) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![
        rmcp::model::ContentBlock::text(json),
    ]))
}

/// 辅助：取 TransferManager 的 Arc（节点未启动返回 None）。
///
/// 克隆 Arc 后立即释放 NetManager 锁，避免持锁跨 await（与 `commands::transfer::get_transfer` 一致）。
async fn resolve_transfer(app: &tauri::AppHandle) -> Option<Arc<TransferManager>> {
    let state = app.state::<NetManagerState>();
    let guard = state.lock().await;
    guard.as_ref().map(|m| m.transfer_arc())
}

/// 辅助：获取 NetManager 锁，未启动时返回 MCP 错误
///
/// 展开为两个 let 绑定，确保 state 和 guard 都在调用者的作用域中存活。
macro_rules! get_net_manager {
    ($handler:expr, $state:ident, $guard:ident) => {
        let $state = $handler.app.state::<NetManagerState>();
        let $guard = $state.lock().await;
        if $guard.is_none() {
            return mcp_error("P2P 网络节点未启动，请先在 SwarmDrop 应用中启动网络");
        }
    };
}

/// 网络状态返回值（MCP 专用简化版）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpNetworkStatus {
    status: String,
    peer_id: Option<String>,
    connected_peers: usize,
    nat_status: String,
    relay_ready: bool,
}

#[tool_router(vis = "pub(super)")]
impl McpHandler {
    /// 获取 P2P 网络状态
    #[tool(
        description = "获取 SwarmDrop P2P 网络节点的运行状态，包括 PeerId、已连接节点数、NAT 类型等",
        annotations(read_only_hint = true)
    )]
    pub async fn get_network_status(&self) -> Result<CallToolResult, ErrorData> {
        let state = self.app.state::<NetManagerState>();
        let guard = state.lock().await;

        let result = match guard.as_ref() {
            Some(manager) => {
                let status = manager.get_network_status();
                McpNetworkStatus {
                    status: "running".into(),
                    peer_id: status.peer_id.map(|p| p.to_string()),
                    connected_peers: status.connected_peers,
                    nat_status: format!("{:?}", status.nat_status),
                    relay_ready: status.relay_ready,
                }
            }
            None => McpNetworkStatus {
                status: "stopped".into(),
                peer_id: None,
                connected_peers: 0,
                nat_status: "Unknown".into(),
                relay_ready: false,
            },
        };

        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        mcp_ok(json)
    }

    /// 列出已配对且在线的设备
    #[tool(
        description = "列出已配对且在线的设备，返回可以发送文件的目标设备列表",
        annotations(read_only_hint = true)
    )]
    pub async fn list_available_devices(&self) -> Result<CallToolResult, ErrorData> {
        get_net_manager!(self, _state, guard);
        let manager = guard.as_ref().unwrap();

        let devices = manager.devices().get_devices(DeviceFilter::Paired);
        let available: Vec<McpDevice> = devices
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Online))
            .map(|d| McpDevice {
                peer_id: d.peer_id.to_string(),
                hostname: d.os_info.hostname,
                os: d.os_info.os,
                platform: d.os_info.platform,
                connection: d.connection.map(|c| format!("{c:?}")),
                latency_ms: d.latency,
            })
            .collect();

        let json = serde_json::to_string_pretty(&available).unwrap_or_default();
        mcp_ok(json)
    }

    /// 向指定设备发送文件
    #[tool(
        description = "向指定设备发送文件。需要提供目标设备的 peer_id（从 list_available_devices 获取）和文件的绝对路径列表",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn send_files(
        &self,
        Parameters(params): Parameters<SendFilesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        get_net_manager!(self, _state2, guard);
        let manager = guard.as_ref().unwrap();

        // 验证文件路径存在并构造 EnumeratedFile 列表
        let mut entries = Vec::new();
        for path_str in &params.file_paths {
            let path = PathBuf::from(path_str);
            if !path.exists() {
                return mcp_error(format!("文件不存在: {path_str}"));
            }

            let meta = tokio::fs::metadata(&path)
                .await
                .map_err(|e| ErrorData::internal_error(format!("读取文件元数据失败: {e}"), None))?;

            if meta.is_dir() {
                let dir_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let source = FileSource::Path { path: path.clone() };
                let dir_files = source
                    .enumerate_dir(&dir_name, &self.app)
                    .await
                    .map_err(|e| ErrorData::internal_error(format!("遍历目录失败: {e}"), None))?;
                entries.extend(dir_files);
            } else {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                entries.push(EnumeratedFile {
                    relative_path: name.clone(),
                    name,
                    source: FileSource::Path { path },
                    size: meta.len(),
                });
            }
        }

        if entries.is_empty() {
            return mcp_error("没有找到可发送的文件");
        }

        // prepare：计算 BLAKE3 hash
        let host_entries: Vec<swarmdrop_core::transfer::HostEnumeratedFile> = entries
            .into_iter()
            .map(|e| swarmdrop_core::transfer::HostEnumeratedFile {
                source_id: crate::host::file_source::source_id(&e.source),
                name: e.name,
                relative_path: e.relative_path,
                size: e.size,
            })
            .collect();
        let prepared_id = uuid::Uuid::new_v4();
        let prepared = manager
            .transfer()
            .prepare(prepared_id, host_entries)
            .await
            .map_err(|e| ErrorData::internal_error(format!("准备传输失败: {e}"), None))?;

        let all_file_ids: Vec<u32> = prepared.files.iter().map(|f| f.file_id).collect();
        let file_count = all_file_ids.len();
        let total_size = prepared.total_size;

        // 查询对端设备名
        let peer_name = manager
            .devices()
            .get_devices(DeviceFilter::Paired)
            .into_iter()
            .find(|d| d.peer_id.to_string() == params.peer_id)
            .map(|d| d.os_info.hostname)
            .unwrap_or_else(|| params.peer_id.clone());

        // send_offer
        let result = manager
            .transfer_arc()
            .send_offer(&prepared_id, &params.peer_id, &peer_name, &all_file_ids)
            .await
            .map_err(|e| ErrorData::internal_error(format!("发送 Offer 失败: {e}"), None))?;

        let response = SendFilesResponse {
            session_id: result.session_id.to_string(),
            file_count,
            total_size,
            message: "Offer 已发送，等待对方在 SwarmDrop 中接受".into(),
        };

        let json = serde_json::to_string_pretty(&response).unwrap_or_default();
        mcp_ok(json)
    }

    /// 检索收件箱（已接收文件）
    #[tool(
        description = "按关键词检索本机已接收的收件箱内容，返回命中条目（标题、来源设备、文件列表含相对路径、接收时间、匹配片段）。先用它定位条目，再用 get_inbox_file 取本地路径。仅覆盖本机 inbox，不跨设备。支持中文（含'合同'这类 2 字词）。默认排除已归档条目，include_archived=true 时纳入。",
        annotations(read_only_hint = true)
    )]
    pub async fn search_inbox(
        &self,
        Parameters(params): Parameters<SearchInboxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，检索暂不可用");
        };
        let limit = params.limit.unwrap_or(20) as usize;
        let hits = match crate::database::inbox::search_inbox(
            &db,
            &params.query,
            limit,
            params.include_archived.unwrap_or(false),
        )
        .await
        {
            Ok(hits) => hits,
            Err(e) => return mcp_error(format!("检索失败: {e}")),
        };
        if hits.is_empty() {
            return mcp_ok("未找到匹配项".to_string());
        }
        let out: Vec<McpInboxHit> = hits.into_iter().map(McpInboxHit::from).collect();
        let json = serde_json::to_string_pretty(&out).unwrap_or_default();
        mcp_ok(json)
    }

    /// 定位收件箱中某个文件的本地路径
    #[tool(
        description = "在检索命中后定位收件箱条目内单个文件的本地路径。需提供 item_id（条目 id）与文件标识：relative_path 或 file_id 二选一（推荐用 search_inbox 命中里的 files[].relativePath）。文件缺失或路径不可达时明确报告，不返回无效路径。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_inbox_file(
        &self,
        Parameters(params): Parameters<GetInboxFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let Ok(item_id) = Uuid::parse_str(&params.item_id) else {
            return mcp_error(format!("无效的条目 id: {}", params.item_id));
        };
        let detail = match crate::database::inbox::get_inbox_item_detail(&db, item_id).await {
            Ok(Some(detail)) => detail,
            Ok(None) => return mcp_error(format!("未找到收件箱条目: {item_id}")),
            Err(e) => return mcp_error(format!("查询失败: {e}")),
        };
        let file = detail.files.iter().find(|file| {
            match (params.relative_path.as_deref(), params.file_id) {
                (Some(rp), _) => file.relative_path == rp,
                (None, Some(fid)) => file.id == fid,
                (None, None) => false,
            }
        });
        let Some(file) = file else {
            return mcp_error("未找到对应文件：请提供有效的 relative_path 或 file_id");
        };
        let missing = file.missing || !std::path::Path::new(&file.local_path).exists();
        let result = McpInboxFile {
            name: file.name.clone(),
            relative_path: file.relative_path.clone(),
            local_path: (!missing).then(|| file.local_path.clone()),
            size: file.size,
            missing,
        };
        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        mcp_ok(json)
    }

    /// 列出进行中与最近的传输会话
    #[tool(
        description = "列出进行中与最近的传输会话（包裹 get_transfer_projections）：sessionId、direction、对端、phase、进度、文件数等。可选 limit（默认 20），按更新时间倒序。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_transfers(
        &self,
        Parameters(params): Parameters<ListTransfersParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let mut projections = match crate::database::ops::get_transfer_projections(&db).await {
            Ok(p) => p,
            Err(e) => return mcp_error(format!("查询失败: {e}")),
        };
        projections.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let limit = params.limit.unwrap_or(20) as usize;
        let out: Vec<McpTransfer> = projections
            .into_iter()
            .take(limit)
            .map(McpTransfer::from)
            .collect();
        let json = serde_json::to_string_pretty(&out).unwrap_or_default();
        mcp_ok(json)
    }

    /// 查询单个传输会话的状态
    #[tool(
        description = "按 sessionId 查询单个传输会话详情：phase、整体进度、分文件状态。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_transfer_status(
        &self,
        Parameters(params): Parameters<TransferSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };
        match crate::database::ops::get_transfer_projection(&db, session_id).await {
            Ok(Some(p)) => {
                let json = serde_json::to_string_pretty(&McpTransfer::from(p)).unwrap_or_default();
                mcp_ok(json)
            }
            Ok(None) => mcp_error(format!("未找到传输会话: {session_id}")),
            Err(e) => mcp_error(format!("查询失败: {e}")),
        }
    }

    /// 取消进行中的传输
    #[tool(
        description = "按 sessionId 取消进行中的传输（通知对端并写入 Cancelled）。",
        annotations(destructive_hint = true)
    )]
    pub async fn cancel_transfer(
        &self,
        Parameters(params): Parameters<TransferSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };
        let Some(transfer) = resolve_transfer(&self.app).await else {
            return mcp_error("P2P 网络节点未启动，请先在 SwarmDrop 应用中启动网络");
        };
        // 取消方向未知：先按发送方取消，失败再按接收方取消（与 commands::transfer::pause_transfer 同模式）。
        match transfer.cancel_send(&session_id).await {
            Ok(()) => mcp_ok(format!("已取消传输 {session_id}")),
            Err(send_err) => match transfer.cancel_receive(&session_id).await {
                Ok(()) => mcp_ok(format!("已取消传输 {session_id}")),
                Err(recv_err) => mcp_error(format!("取消失败: {send_err}; {recv_err}")),
            },
        }
    }

    /// 暂停进行中的传输
    #[tool(description = "按 sessionId 暂停进行中的传输。")]
    pub async fn pause_transfer(
        &self,
        Parameters(params): Parameters<TransferSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };
        let Some(transfer) = resolve_transfer(&self.app).await else {
            return mcp_error("P2P 网络节点未启动，请先在 SwarmDrop 应用中启动网络");
        };
        match transfer.pause_send(&session_id).await {
            Ok(()) => mcp_ok(format!("已暂停传输 {session_id}")),
            Err(send_err) => match transfer.pause_receive(&session_id).await {
                Ok(()) => mcp_ok(format!("已暂停传输 {session_id}")),
                Err(recv_err) => mcp_error(format!("暂停失败: {send_err}; {recv_err}")),
            },
        }
    }

    /// 恢复已暂停的传输
    #[tool(
        description = "按 sessionId 恢复已暂停的传输（走 Probe→Commit→Ack）。对端不可用时会话保留 suspended 供稍后重试。"
    )]
    pub async fn resume_transfer(
        &self,
        Parameters(params): Parameters<TransferSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };
        let Some(transfer) = resolve_transfer(&self.app).await else {
            return mcp_error("P2P 网络节点未启动，请先在 SwarmDrop 应用中启动网络");
        };
        match transfer.initiate_resume(session_id).await {
            Ok(info) => {
                let result = McpResumeResult {
                    session_id: session_id.to_string(),
                    peer_name: info.peer_name,
                    file_count: info.files.len(),
                    total_size: info.total_size,
                    transferred_bytes: info.transferred_bytes,
                    message: "已发起恢复，正在与对端协商续传".into(),
                };
                let json = serde_json::to_string_pretty(&result).unwrap_or_default();
                mcp_ok(json)
            }
            Err(e) => mcp_error(format!("恢复失败: {e}")),
        }
    }

    /// 列出收件箱条目（无需关键词）
    #[tool(
        description = "按接收时间倒序列出收件箱条目（无需关键词），与 search_inbox 互补。可选 limit（默认 20）与 include_archived（默认 false）。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_inbox(
        &self,
        Parameters(params): Parameters<ListInboxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let include_archived = params.include_archived.unwrap_or(false);
        let items = match crate::database::inbox::list_inbox_items(&db, include_archived).await {
            Ok(items) => items,
            Err(e) => return mcp_error(format!("查询失败: {e}")),
        };
        if items.is_empty() {
            return mcp_ok("收件箱为空".to_string());
        }
        let limit = params.limit.unwrap_or(20) as usize;
        let out: Vec<McpInboxItem> = items
            .into_iter()
            .take(limit)
            .map(McpInboxItem::from)
            .collect();
        let json = serde_json::to_string_pretty(&out).unwrap_or_default();
        mcp_ok(json)
    }

    /// 取收件箱条目完整详情
    #[tool(
        description = "按条目 id 取完整详情（标题、来源、接收时间、文件列表含 relativePath/size/missing/localPath），补全 search_inbox/list_inbox → 详情 → get_inbox_file 的闭环。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_inbox_item(
        &self,
        Parameters(params): Parameters<GetInboxItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let Ok(item_id) = Uuid::parse_str(&params.item_id) else {
            return mcp_error(format!("无效的条目 id: {}", params.item_id));
        };
        let detail = match crate::database::inbox::get_inbox_item_detail(&db, item_id).await {
            Ok(Some(detail)) => detail,
            Ok(None) => return mcp_error(format!("未找到收件箱条目: {item_id}")),
            Err(e) => return mcp_error(format!("查询失败: {e}")),
        };
        let json =
            serde_json::to_string_pretty(&McpInboxItemDetail::from(detail)).unwrap_or_default();
        mcp_ok(json)
    }
}

/// send_files 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SendFilesParams {
    /// 目标设备的 PeerId（从 list_available_devices 获取）
    pub peer_id: String,
    /// 要发送的文件/目录的绝对路径列表
    pub file_paths: Vec<String>,
}

/// send_files 的返回值
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SendFilesResponse {
    session_id: String,
    file_count: usize,
    total_size: u64,
    message: String,
}

/// 简化的设备信息（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpDevice {
    peer_id: String,
    hostname: String,
    os: String,
    platform: String,
    connection: Option<String>,
    latency_ms: Option<u64>,
}

/// search_inbox 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchInboxParams {
    /// 检索关键词（支持中文，含 2 字词如"合同"）
    pub query: String,
    /// 返回条数上限，默认 20
    pub limit: Option<u32>,
    /// 是否纳入已归档条目，默认 false
    pub include_archived: Option<bool>,
}

/// get_inbox_file 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetInboxFileParams {
    /// 收件箱条目 id（来自 search_inbox 命中的 id）
    pub item_id: String,
    /// 文件相对路径（来自 search_inbox 命中的 files[].relativePath）
    pub relative_path: Option<String>,
    /// 文件 id（与 relative_path 二选一）
    pub file_id: Option<i32>,
}

/// search_inbox 命中（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxHit {
    id: String,
    title: String,
    source_name: String,
    item_count: i32,
    received_at: i64,
    snippet: String,
    files: Vec<McpInboxHitFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxHitFile {
    name: String,
    relative_path: String,
}

impl From<crate::database::inbox::InboxSearchHit> for McpInboxHit {
    fn from(hit: crate::database::inbox::InboxSearchHit) -> Self {
        Self {
            id: hit.id.to_string(),
            title: hit.title,
            source_name: hit.source_name,
            item_count: hit.item_count,
            received_at: hit.received_at,
            snippet: hit.snippet,
            files: hit
                .files
                .into_iter()
                .map(|file| McpInboxHitFile {
                    name: file.name,
                    relative_path: file.relative_path,
                })
                .collect(),
        }
    }
}

/// get_inbox_file 返回（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxFile {
    name: String,
    relative_path: String,
    /// 文件存在时的本地绝对路径；缺失时为 null
    local_path: Option<String>,
    size: i64,
    missing: bool,
}

/// list_transfers 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTransfersParams {
    /// 返回条数上限，默认 20
    pub limit: Option<u32>,
}

/// 单会话操作的通用输入参数（get_transfer_status / cancel / pause / resume）
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TransferSessionParams {
    /// 传输会话 id（来自 list_transfers）
    pub session_id: String,
}

/// list_inbox 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListInboxParams {
    /// 返回条数上限，默认 20
    pub limit: Option<u32>,
    /// 是否纳入已归档条目，默认 false
    pub include_archived: Option<bool>,
}

/// get_inbox_item 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetInboxItemParams {
    /// 收件箱条目 id（来自 search_inbox / list_inbox 命中的 id）
    pub item_id: String,
}

/// 传输会话投影（MCP 输出，裁剪掉 epoch / bitmap / savePath 等内部字段）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpTransfer {
    session_id: String,
    direction: String,
    peer_id: String,
    peer_name: String,
    phase: String,
    reason: Option<String>,
    recoverable: bool,
    total_size: i64,
    transferred_bytes: i64,
    file_count: usize,
    started_at: i64,
    updated_at: i64,
    finished_at: Option<i64>,
    error_message: Option<String>,
    files: Vec<McpTransferFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpTransferFile {
    file_id: i32,
    name: String,
    relative_path: String,
    size: i64,
    transferred_bytes: i64,
}

impl From<crate::database::ops::TransferProjection> for McpTransfer {
    fn from(p: crate::database::ops::TransferProjection) -> Self {
        let direction = match p.direction {
            entity::TransferDirection::Send => "send",
            entity::TransferDirection::Receive => "receive",
        };
        let reason = p
            .suspended_reason
            .map(|r| format!("{r:?}"))
            .or_else(|| p.terminal_reason.map(|r| format!("{r:?}")));
        Self {
            session_id: p.session_id.to_string(),
            direction: direction.into(),
            peer_id: p.peer_id,
            peer_name: p.peer_name,
            phase: format!("{:?}", p.phase),
            reason,
            recoverable: p.recoverable,
            total_size: p.total_size,
            transferred_bytes: p.transferred_bytes,
            file_count: p.files.len(),
            started_at: p.started_at,
            updated_at: p.updated_at,
            finished_at: p.finished_at,
            error_message: p.error_message,
            files: p
                .files
                .into_iter()
                .map(|f| McpTransferFile {
                    file_id: f.file_id,
                    name: f.name,
                    relative_path: f.relative_path,
                    size: f.size,
                    transferred_bytes: f.transferred_bytes,
                })
                .collect(),
        }
    }
}

/// resume_transfer 返回（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpResumeResult {
    session_id: String,
    peer_name: String,
    file_count: usize,
    total_size: i64,
    transferred_bytes: i64,
    message: String,
}

/// 收件箱条目摘要（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxItem {
    id: String,
    title: String,
    source_name: String,
    item_count: i32,
    total_size: i64,
    received_at: i64,
    archived: bool,
    missing: bool,
}

impl From<crate::database::inbox::InboxItemSummary> for McpInboxItem {
    fn from(s: crate::database::inbox::InboxItemSummary) -> Self {
        Self {
            id: s.id.to_string(),
            title: s.title,
            source_name: s.source_name,
            item_count: s.item_count,
            total_size: s.total_size,
            received_at: s.received_at,
            archived: s.archived_at.is_some(),
            missing: s.missing,
        }
    }
}

/// 收件箱条目详情（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxItemDetail {
    #[serde(flatten)]
    item: McpInboxItem,
    files: Vec<McpInboxDetailFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpInboxDetailFile {
    id: i32,
    name: String,
    relative_path: String,
    size: i64,
    /// 文件存在时的本地绝对路径；缺失时为 null
    local_path: Option<String>,
    missing: bool,
}

impl From<crate::database::inbox::InboxItemDetail> for McpInboxItemDetail {
    fn from(detail: crate::database::inbox::InboxItemDetail) -> Self {
        let files = detail
            .files
            .into_iter()
            .map(|f| {
                let missing = f.missing || !std::path::Path::new(&f.local_path).exists();
                McpInboxDetailFile {
                    id: f.id,
                    name: f.name,
                    relative_path: f.relative_path,
                    size: f.size,
                    local_path: (!missing).then(|| f.local_path.clone()),
                    missing,
                }
            })
            .collect();
        Self {
            item: McpInboxItem::from(detail.item),
            files,
        }
    }
}
