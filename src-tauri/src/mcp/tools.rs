//! MCP Tool 实现
//!
//! 提供 3 个 Tool：get_network_status、list_available_devices、send_files

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, schemars, tool, tool_router};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
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
        description = "获取 SwarmDrop P2P 网络节点的运行状态，包括 PeerId、已连接节点数、NAT 类型等"
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
    #[tool(description = "列出已配对且在线的设备，返回可以发送文件的目标设备列表")]
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
        description = "向指定设备发送文件。需要提供目标设备的 peer_id（从 list_available_devices 获取）和文件的绝对路径列表"
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
        description = "按关键词检索本机已接收的收件箱内容，返回命中条目（标题、来源设备、文件列表含相对路径、接收时间、匹配片段）。先用它定位条目，再用 get_inbox_file 取本地路径。仅覆盖本机 inbox，不跨设备。支持中文（含'合同'这类 2 字词）。"
    )]
    pub async fn search_inbox(
        &self,
        Parameters(params): Parameters<SearchInboxParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，检索暂不可用");
        };
        let limit = params.limit.unwrap_or(20) as usize;
        let hits =
            match crate::database::inbox::search_inbox(&db, &params.query, limit, false).await {
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
        description = "在检索命中后定位收件箱条目内单个文件的本地路径。需提供 item_id（条目 id）与文件标识：relative_path 或 file_id 二选一（推荐用 search_inbox 命中里的 files[].relativePath）。文件缺失或路径不可达时明确报告，不返回无效路径。"
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
