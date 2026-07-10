//! MCP Tool 实现
//!
//! 提供 20 个 Tool：
//! - 网络/发送：get_network_status、list_available_devices、send_files
//! - 节点/接收生命周期：ensure_node_running、get_receiving_paused、set_receiving_paused
//! - 传输生命周期：list_transfers、get_transfer_status、cancel_transfer、pause_transfer、resume_transfer
//! - 入站代收：accept_transfer、reject_transfer
//! - 设备（只读）：list_paired_devices
//! - 收件箱：search_inbox、list_inbox、get_inbox_item、get_inbox_file、archive_inbox_item、export_inbox_item

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

/// 辅助：定位挂起入站 offer 并校验 MCP 代收门控（accept_transfer / reject_transfer 共用）。
///
/// 成功返回 `(transfer Arc, 来源设备名)`；失败返回**已构造好**的 MCP 错误结果供调用方直接 `return`。
/// 取完所需信息即释放 NetManager 锁，不持锁跨调用方后续的落盘 / 回复 await。
async fn gate_pending_offer(
    app: &tauri::AppHandle,
    session_id: &Uuid,
) -> Result<(Arc<TransferManager>, String), Result<CallToolResult, ErrorData>> {
    let state = app.state::<NetManagerState>();
    let guard = state.lock().await;
    let Some(manager) = guard.as_ref() else {
        return Err(mcp_error(
            "P2P 网络节点未启动，请先在 SwarmDrop 应用中启动网络",
        ));
    };
    let transfer = manager.transfer_arc();
    let Some(peer_id) = transfer.pending_offer_peer(session_id) else {
        return Err(mcp_error(
            "传输会话不存在或已过期：可能已被处理，或已超过约 3 分钟的确认窗口",
        ));
    };
    // 门控：来源设备须开启 allow_mcp_accept_from_device（镜像发送侧 allow_mcp_send_to_device）。
    let device = manager.pairing().get_paired_device(&peer_id);
    let allowed = device
        .as_ref()
        .map(|d| d.receive_policy.allow_mcp_accept_from_device)
        .unwrap_or(false);
    let source_name = device
        .map(|d| d.os_info.hostname)
        .unwrap_or_else(|| peer_id.to_string());
    if !allowed {
        return Err(mcp_error(format!(
            "来源设备「{source_name}」未开启「允许 MCP 代收」；请在 SwarmDrop 的设备策略中开启后重试"
        )));
    }
    Ok((transfer, source_name))
}

/// 辅助：解析 agent 代收的默认收件目录。**与手动接收共用同一个「接收文件夹」**——优先读用户
/// 在设置里配的 `preferences.transfer.savePath`，未配则回退 `<下载目录>/SwarmDrop`（与前端
/// `getDefaultSavePath` 一致）。不搞独立 agent 子目录：代收文件落在同一处，靠收件箱的
/// `origin=mcp`「AI 代理」标记区分来源（见 `accept_transfer`）。不存在则创建，返回绝对路径。
fn mcp_default_receive_dir(app: &tauri::AppHandle) -> Result<String, String> {
    let dir = match read_persisted_save_path(app) {
        Some(configured) => std::path::PathBuf::from(configured),
        None => app
            .path()
            .download_dir()
            .map_err(|e| format!("下载目录不可用: {e}"))?
            .join("SwarmDrop"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 读前端持久化的接收文件夹（`preferences.transfer.savePath`）。zustand persist 把整个 state
/// `JSON.stringify` 后再 `store.set`，故取出是字符串、需再 `from_str` 一次（与 i18n locale
/// 读取同源）。空串视为未配置。
fn read_persisted_save_path(app: &tauri::AppHandle) -> Option<String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("preferences.json").ok()?;
    let raw = store.get("preferences-store")?;
    let parsed: serde_json::Value = serde_json::from_str(raw.as_str()?).ok()?;
    let path = parsed
        .get("state")?
        .get("transfer")?
        .get("savePath")?
        .as_str()?;
    (!path.is_empty()).then(|| path.to_string())
}

/// `ensure_node_running` 的启动互斥。
///
/// MCP 是无人值守的并发调用方：两个 `ensure_node_running` 若同时看到「未运行」会双双调
/// `start`，后者覆盖前者、泄漏首个节点的后台任务 / event loop。用进程级互斥串行化「检查是否
/// 运行 + 启动」这段临界区——第二个调用者拿锁后已能看到运行中，直接走幂等返回。
fn node_start_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
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
        description = "列出已配对且在线的设备，返回可以发送文件的目标设备列表。displayName 是面向用户的设备名（优先用户设置，未设置时回退 hostname）；peerId 仅用于后续 send_files 调用。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_available_devices(&self) -> Result<CallToolResult, ErrorData> {
        get_net_manager!(self, _state, guard);
        let manager = guard.as_ref().unwrap();

        let devices = manager.devices().get_devices(DeviceFilter::Paired);
        let available: Vec<McpDevice> = devices
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Online))
            .map(McpDevice::from)
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
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        get_net_manager!(self, _state2, guard);
        let manager = guard.as_ref().unwrap();

        // 发送端门控：MCP 来源需目标设备策略放行（allow_mcp_send_to_device）。
        // 这是真正的发送侧安全控制——防止 agent 静默把文件外传到未授权设备。
        if let Ok(target_peer) = params.peer_id.parse::<swarm_p2p_core::libp2p::PeerId>()
            && let Some(device) = manager.pairing().get_paired_device(&target_peer)
            && !device.receive_policy.allow_mcp_send_to_device
        {
            return mcp_error(format!(
                "目标设备「{}」的策略不允许 MCP/AI 发送；请在 SwarmDrop 的设备策略中开启「允许 MCP 发送」",
                device.os_info.hostname
            ));
        }

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

        // send_offer：MCP 来源，尽力带上 initialize 握手报告的客户端名（如 claude-desktop）。
        let client = context
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone());
        let result = manager
            .transfer_arc()
            .send_offer(
                &prepared_id,
                &params.peer_id,
                &peer_name,
                &all_file_ids,
                swarmdrop_core::protocol::TransferOrigin::Mcp { client },
            )
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
        projections.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
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

    /// 接受一个挂起的入站文件 offer（代收）
    #[tool(
        description = "接受一个处于 RequireConfirmation 挂起态的入站文件 offer——这类会话在 list_transfers 里表现为 direction=receive、phase=Offered，先用它发现待审 offer 再取 sessionId。需来源设备已在 SwarmDrop 设备策略中开启「允许 MCP 代收」，否则被门控拒绝。可选 save_path（绝对目录）指定落盘位置；缺省落到与手动接收一致的接收文件夹（设置里的接收位置，未配则 <下载目录>/SwarmDrop）。代收的文件会在收件箱标记为「AI 代理」来源，便于与手动接收区分。注意：挂起 offer 的有效决策窗口约 3 分钟（受 libp2p 协议超时封顶），要可靠代收需让 agent 处于活跃轮询循环，别指望被动唤醒。",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn accept_transfer(
        &self,
        Parameters(params): Parameters<AcceptTransferParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };

        // 定位挂起 offer + 校验代收门控（内部取完即释放 NetManager 锁，不持锁跨 await）
        let (transfer, _source) = match gate_pending_offer(&self.app, &session_id).await {
            Ok(v) => v,
            Err(err) => return err,
        };

        // 解析保存位置：显式 save_path 或 MCP 默认收件目录
        let save_location = match params.save_path.as_deref() {
            Some(path) => swarmdrop_core::host::CoreSaveLocation::Path {
                path: path.to_string(),
            },
            None => match mcp_default_receive_dir(&self.app) {
                Ok(dir) => swarmdrop_core::host::CoreSaveLocation::Path { path: dir },
                Err(e) => return mcp_error(format!("无法解析 MCP 默认收件目录: {e}")),
            },
        };
        let swarmdrop_core::host::CoreSaveLocation::Path { path: save_path } =
            save_location.clone();

        match transfer
            .accept_and_start_receive(&session_id, save_location)
            .await
        {
            Ok(()) => {
                // 标记为 MCP 代收：把 session origin 更新为 Mcp{client}，使完成后建的收件箱
                // 条目 source_kind=mcp（UI 显示「AI 代理」）。与落盘位置无关——文件仍落在与手动
                // 一致的接收文件夹，靠此标记区分。尽力带上 initialize 握手报告的客户端名。
                let client = context
                    .peer
                    .peer_info()
                    .map(|info| info.client_info.name.clone());
                if let Some(db) = self.app.try_state::<DatabaseConnection>() {
                    let _ = swarmdrop_core::database::ops::update_session_origin(
                        &db,
                        session_id,
                        swarmdrop_core::protocol::TransferOrigin::Mcp { client },
                    )
                    .await;
                }
                let response = serde_json::json!({
                    "sessionId": session_id.to_string(),
                    "savePath": save_path,
                    "message": "已接受入站传输，开始接收",
                });
                mcp_ok(serde_json::to_string_pretty(&response).unwrap_or_default())
            }
            Err(e) => mcp_error(format!("接受传输失败: {e}")),
        }
    }

    /// 拒绝一个挂起的入站文件 offer（代收）
    #[tool(
        description = "拒绝一个处于 RequireConfirmation 挂起态的入站文件 offer（direction=receive、phase=Offered，用 list_transfers 发现）。需来源设备已开启「允许 MCP 代收」。会通知对端并婉拒该传输。",
        annotations(destructive_hint = true)
    )]
    pub async fn reject_transfer(
        &self,
        Parameters(params): Parameters<TransferSessionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(session_id) = Uuid::parse_str(&params.session_id) else {
            return mcp_error(format!("无效的 session_id: {}", params.session_id));
        };
        let (transfer, _source) = match gate_pending_offer(&self.app, &session_id).await {
            Ok(v) => v,
            Err(err) => return err,
        };
        match transfer.reject_and_respond(&session_id).await {
            Ok(()) => mcp_ok(format!("已拒绝入站传输 {session_id}")),
            Err(e) => mcp_error(format!("拒绝失败: {e}")),
        }
    }

    /// 查询全局「暂停接收」状态
    #[tool(
        description = "查询本机是否处于全局「暂停接收」状态。暂停时新入站 offer 会被自动婉拒——若你发现不到入站 offer，先用它确认是否被暂停。",
        annotations(read_only_hint = true)
    )]
    pub async fn get_receiving_paused(&self) -> Result<CallToolResult, ErrorData> {
        let paused = crate::tray::current_receiving_paused(&self.app).await;
        mcp_ok(serde_json::json!({ "paused": paused }).to_string())
    }

    /// 设置全局「暂停接收」
    #[tool(
        description = "开关本机全局「暂停接收」。暂停仅对新入站 offer 自动婉拒，不影响节点在线/配对/发现。典型用途：批量处理前先静音、处理完恢复。",
        annotations(read_only_hint = false)
    )]
    pub async fn set_receiving_paused(
        &self,
        Parameters(params): Parameters<SetReceivingPausedParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match crate::tray::apply_receiving_paused(&self.app, params.paused).await {
            Ok(()) => mcp_ok(serde_json::json!({ "paused": params.paused }).to_string()),
            Err(e) => mcp_error(format!("设置暂停接收失败: {e}")),
        }
    }

    /// 列出全部已配对设备（只读，含策略标志）
    #[tool(
        description = "只读列出全部已配对设备（含离线），带用户设备名（displayName）、在线态、信任级别与 MCP 策略标志（allowMcpSendToDevice / allowMcpAcceptFromDevice / autoAccept）。用于解释某设备为何发不出 / 收不了。与 list_available_devices（仅在线可发送）互补。",
        annotations(read_only_hint = true)
    )]
    pub async fn list_paired_devices(&self) -> Result<CallToolResult, ErrorData> {
        get_net_manager!(self, _state, guard);
        let manager = guard.as_ref().unwrap();
        let devices = manager.devices().get_devices(DeviceFilter::Paired);
        let out: Vec<McpPairedDevice> = devices.into_iter().map(McpPairedDevice::from).collect();
        mcp_ok(serde_json::to_string_pretty(&out).unwrap_or_default())
    }

    /// 归档 / 取消归档收件箱条目
    #[tool(
        description = "归档或取消归档一个收件箱条目（可逆）。archived=true 归档、false 取消归档。",
        annotations(read_only_hint = false)
    )]
    pub async fn archive_inbox_item(
        &self,
        Parameters(params): Parameters<ArchiveInboxItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let Ok(item_id) = Uuid::parse_str(&params.item_id) else {
            return mcp_error(format!("无效的条目 id: {}", params.item_id));
        };
        match crate::database::inbox::archive_inbox_item(&db, item_id, params.archived).await {
            Ok(()) => mcp_ok(format!(
                "已{}收件箱条目 {item_id}",
                if params.archived {
                    "归档"
                } else {
                    "取消归档"
                }
            )),
            Err(e) => mcp_error(format!("操作失败: {e}")),
        }
    }

    /// 导出收件箱条目到指定目录
    #[tool(
        description = "把一个收件箱条目的文件复制导出到指定目录（destination_dir，绝对路径，不存在则创建）。用于把收到的内容投递到目标位置。",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn export_inbox_item(
        &self,
        Parameters(params): Parameters<ExportInboxItemParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(db) = self.app.try_state::<DatabaseConnection>() else {
            return mcp_error("数据库尚未就绪，暂不可用");
        };
        let Ok(item_id) = Uuid::parse_str(&params.item_id) else {
            return mcp_error(format!("无效的条目 id: {}", params.item_id));
        };
        // 委托既有命令，别在此内联重写导出逻辑：命令经 `ensure_file_exists`，文件缺失时会
        // 回写 DB 的 missing 标志、保持 DB 与磁盘一致——内联版漏了这个副作用。
        match crate::commands::export_inbox_item(db, item_id, params.destination_dir.clone()).await
        {
            Ok(()) => mcp_ok(
                serde_json::json!({
                    "itemId": item_id.to_string(),
                    "destinationDir": params.destination_dir,
                })
                .to_string(),
            ),
            Err(e) => mcp_error(format!("导出失败: {e}")),
        }
    }

    /// 确保本机 P2P 节点已上线
    #[tool(
        description = "确保本机 P2P 节点已上线：已运行则幂等返回当前网络状态；未运行则自动启动（从 keychain 自取已配对设备 + 默认网络设置）。需设备身份已在 SwarmDrop 中解锁——身份未就绪时报错提示到 app 解锁。不提供停止节点的能力（下线是用户级操作）。",
        annotations(read_only_hint = false, open_world_hint = true)
    )]
    pub async fn ensure_node_running(&self) -> Result<CallToolResult, ErrorData> {
        // 串行化「检查是否运行 + 启动」临界区，避免并发调用双双启动（见 node_start_lock）。
        let _start_guard = node_start_lock().lock().await;

        // 幂等：已运行直接返回状态（取完即释放锁）
        {
            let state = self.app.state::<NetManagerState>();
            let guard = state.lock().await;
            if let Some(manager) = guard.as_ref() {
                let status = manager.get_network_status();
                return mcp_ok(
                    serde_json::json!({
                        "status": "running",
                        "alreadyRunning": true,
                        "peerId": status.peer_id.map(|p| p.to_string()),
                        "connectedPeers": status.connected_peers,
                    })
                    .to_string(),
                );
            }
        }

        // 门控：身份已解锁（keypair 已 manage 进 state）才允许 agent 拉起节点。
        let Some(keypair) = self
            .app
            .try_state::<swarm_p2p_core::libp2p::identity::Keypair>()
        else {
            return mcp_error("设备身份未解锁，请先在 SwarmDrop 应用中解锁后重试");
        };

        // 复用既有 start：内部从 keychain 自取已配对设备；network_options=None 走默认设置。
        match crate::commands::start(self.app.clone(), keypair, Vec::new(), None).await {
            Ok(()) => {
                let state = self.app.state::<NetManagerState>();
                let guard = state.lock().await;
                let status = guard.as_ref().map(|m| m.get_network_status());
                mcp_ok(
                    serde_json::json!({
                        "status": "running",
                        "alreadyRunning": false,
                        "peerId": status.and_then(|s| s.peer_id).map(|p| p.to_string()),
                        "message": "已启动 P2P 节点",
                    })
                    .to_string(),
                )
            }
            Err(e) => mcp_error(format!("启动节点失败: {e}")),
        }
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

/// accept_transfer 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AcceptTransferParams {
    /// 待接受的传输会话 id（来自 list_transfers 中 direction=receive、phase=Offered 的会话）
    pub session_id: String,
    /// 可选：文件落盘的绝对目录路径。缺省时落到 MCP 专用默认收件目录（下载目录下的 SwarmDrop）
    pub save_path: Option<String>,
}

/// set_receiving_paused 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetReceivingPausedParams {
    /// true=暂停接收，false=恢复接收
    pub paused: bool,
}

/// archive_inbox_item 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ArchiveInboxItemParams {
    /// 收件箱条目 id
    pub item_id: String,
    /// true=归档，false=取消归档
    pub archived: bool,
}

/// export_inbox_item 的输入参数
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportInboxItemParams {
    /// 收件箱条目 id
    pub item_id: String,
    /// 导出目标目录（绝对路径，不存在则创建）
    pub destination_dir: String,
}

/// 已配对设备（MCP 只读输出，含策略标志）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpPairedDevice {
    peer_id: String,
    /// 用户设置的设备名称；未设置时为 null。
    name: Option<String>,
    /// 面向用户显示的设备名，优先使用 name，未设置时回退 hostname。
    display_name: String,
    hostname: String,
    os: String,
    platform: String,
    online: bool,
    trust_level: Option<String>,
    allow_mcp_send_to_device: bool,
    allow_mcp_accept_from_device: bool,
    auto_accept: bool,
}

impl From<crate::device::Device> for McpPairedDevice {
    fn from(d: crate::device::Device) -> Self {
        let policy = d.receive_policy;
        let display_name = mcp_display_name(d.os_info.name.as_deref(), &d.os_info.hostname);
        Self {
            peer_id: d.peer_id.to_string(),
            name: d.os_info.name,
            display_name,
            hostname: d.os_info.hostname,
            os: d.os_info.os,
            platform: d.os_info.platform,
            online: matches!(d.status, DeviceStatus::Online),
            trust_level: d.trust_level.map(|t| format!("{t:?}")),
            allow_mcp_send_to_device: policy
                .as_ref()
                .map(|p| p.allow_mcp_send_to_device)
                .unwrap_or(false),
            allow_mcp_accept_from_device: policy
                .as_ref()
                .map(|p| p.allow_mcp_accept_from_device)
                .unwrap_or(false),
            auto_accept: policy.as_ref().map(|p| p.auto_accept).unwrap_or(false),
        }
    }
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

/// 面向 MCP 的设备显示名：用户命名优先，系统 hostname 仅作回退。
fn mcp_display_name(name: Option<&str>, hostname: &str) -> String {
    name.filter(|name| !name.trim().is_empty())
        .unwrap_or(hostname)
        .to_owned()
}

/// 简化的可用设备信息（MCP 输出）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpDevice {
    peer_id: String,
    /// 用户设置的设备名称；未设置时为 null。
    name: Option<String>,
    /// 面向用户显示的设备名，优先使用 name，未设置时回退 hostname。
    display_name: String,
    hostname: String,
    os: String,
    platform: String,
    connection: Option<String>,
    latency_ms: Option<u64>,
}

impl From<crate::device::Device> for McpDevice {
    fn from(d: crate::device::Device) -> Self {
        let display_name = mcp_display_name(d.os_info.name.as_deref(), &d.os_info.hostname);
        Self {
            peer_id: d.peer_id.to_string(),
            name: d.os_info.name,
            display_name,
            hostname: d.os_info.hostname,
            os: d.os_info.os,
            platform: d.os_info.platform,
            connection: d.connection.map(|connection| format!("{connection:?}")),
            latency_ms: d.latency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mcp_display_name;

    #[test]
    fn mcp_display_name_prefers_user_supplied_name() {
        assert_eq!(
            mcp_display_name(Some("我的 MacBook"), "DESKTOP-1234"),
            "我的 MacBook"
        );
    }

    #[test]
    fn mcp_display_name_falls_back_when_name_is_blank() {
        assert_eq!(mcp_display_name(Some("  "), "DESKTOP-1234"), "DESKTOP-1234");
    }
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
