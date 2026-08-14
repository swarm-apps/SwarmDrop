//! 文件传输 —— 委托给 [`swarmdrop_core::transfer::manager::TransferManager`]。
//!
//! mobile-core 这层只做：
//! - uniffi Record 镜像（输入/输出类型）
//! - String <-> Uuid / PeerId / SaveLocation 等 ID 转换
//!
//! 业务逻辑（hash、加密、断点续传、DB checkpoint）全部在 core，
//! 文件 I/O 通过 `ForeignFileAccess` callback 走 RN 的 expo-file-system。

use swarmdrop_core::transfer::HostEnumeratedFile;
use swarmdrop_core::transfer::incoming::{TransferOfferEvent, TransferOfferFileEvent};
use uuid::Uuid;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

// ─────────────── 输入/输出 Record ───────────────

/// 待发送的单个文件（RN 调 prepare_send 时构造）
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferFile {
    /// host 自定义的 source 标识（RN 用 expo-fs uri）
    pub source_id: String,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

/// 接收方收到的 Offer 中的单个文件
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOfferFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: Option<String>,
    pub size: u64,
    pub is_directory: bool,
}

impl From<TransferOfferFileEvent> for MobileTransferOfferFile {
    fn from(file: TransferOfferFileEvent) -> Self {
        // 穷尽解构（drift guard）：core 给 TransferOfferFileEvent 加字段时这里会编译失败。
        let TransferOfferFileEvent {
            file_id,
            name,
            relative_path,
            size,
            is_directory,
        } = file;
        Self {
            file_id,
            name,
            // core 用空字符串表示根目录文件；FFI 收敛到 Option
            relative_path: if relative_path.is_empty() {
                None
            } else {
                Some(relative_path)
            },
            size,
            is_directory,
        }
    }
}

/// 传输发起来源（人工 / MCP 代理），镜像 core `TransferOrigin`。
#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileTransferOrigin {
    Human,
    Mcp { client: Option<String> },
}

impl From<swarmdrop_core::protocol::TransferOrigin> for MobileTransferOrigin {
    fn from(origin: swarmdrop_core::protocol::TransferOrigin) -> Self {
        match origin {
            swarmdrop_core::protocol::TransferOrigin::Human => Self::Human,
            swarmdrop_core::protocol::TransferOrigin::Mcp { client } => Self::Mcp { client },
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOffer {
    pub session_id: String,
    pub peer_id: String,
    pub device_name: String,
    pub files: Vec<MobileTransferOfferFile>,
    pub total_size: u64,
    pub origin: MobileTransferOrigin,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
}

impl From<TransferOfferEvent> for MobileTransferOffer {
    fn from(offer: TransferOfferEvent) -> Self {
        // 穷尽解构（drift guard）：core 给 TransferOfferEvent 加字段时这里会编译失败。
        let TransferOfferEvent {
            session_id,
            peer_id,
            device_name,
            files,
            total_size,
            origin,
            policy_action,
            policy_reason,
        } = offer;
        Self {
            session_id: session_id.to_string(),
            peer_id,
            device_name,
            files: files.into_iter().map(Into::into).collect(),
            total_size,
            origin: origin.into(),
            policy_action,
            policy_reason,
        }
    }
}

/// prepare_send 的返回类型
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedTransfer {
    pub prepared_id: String,
    pub total_size: u64,
    pub files: Vec<MobilePreparedFile>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedFile {
    pub file_id: u32,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileSendResult {
    pub session_id: String,
}

// ─────────────── MobileCore 方法 ───────────────

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    /// 准备发送：流式 BLAKE3 hash + 通过 EventBus 推 PrepareProgress 事件
    pub async fn prepare_send(
        &self,
        files: Vec<MobileTransferFile>,
    ) -> FfiResult<MobilePreparedTransfer> {
        if files.is_empty() {
            return Err(FfiError::Transfer("no files selected".into()));
        }
        let manager = self.transfer_manager_arc().await?;

        let prepared_id = Uuid::new_v4();
        let host_files: Vec<HostEnumeratedFile> = files
            .into_iter()
            .map(|f| HostEnumeratedFile {
                source_id: swarmdrop_core::host::FileSourceId(f.source_id),
                name: f.name,
                relative_path: f.relative_path,
                size: f.size,
            })
            .collect();

        let prepared = manager.prepare(prepared_id, host_files).await?;
        Ok(MobilePreparedTransfer {
            prepared_id: prepared.prepared_id.to_string(),
            total_size: prepared.total_size,
            files: prepared
                .files
                .iter()
                .map(|f| MobilePreparedFile {
                    file_id: f.file_id,
                    name: f.name.clone(),
                    relative_path: f.relative_path.clone(),
                    size: f.size,
                })
                .collect(),
        })
    }

    /// 发送：构造 Offer 给对端（异步，结果通过 TransferAccepted/Rejected/Failed 事件回报）
    pub async fn send_prepared(
        &self,
        prepared_id: String,
        peer_id: String,
        peer_name: String,
        file_ids: Vec<u32>,
    ) -> FfiResult<MobileSendResult> {
        parse_peer_id(&peer_id)?;
        let prepared_uuid = Uuid::parse_str(&prepared_id)
            .map_err(|_| FfiError::Transfer(format!("invalid prepared_id: {prepared_id}")))?;
        let manager = self.transfer_manager_arc().await?;
        let result = manager
            .send_offer(
                &prepared_uuid,
                &peer_id,
                &peer_name,
                &file_ids,
                // 移动端发送由用户在 App 内手动发起。
                swarmdrop_core::protocol::TransferOrigin::Human,
            )
            .await
            .map_err(FfiError::from)?;
        Ok(MobileSendResult {
            session_id: result.session_id.to_string(),
        })
    }

    /// 接受接收：保存到 RN 提供的目录
    ///
    /// `save_location_uri` 是 RN 端把文件写到哪的标识（比如 expo-fs 的 documentDirectory）。
    /// 实际写入路径由 ForeignFileAccess::create_sink 实现决定，core 只把 uri 当作 SaveLocation::Path 透传。
    pub async fn accept_receive(
        &self,
        session_id: String,
        save_location_uri: String,
    ) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .accept_and_start_receive(
                &session_uuid,
                swarmdrop_core::host::CoreSaveLocation::Path {
                    path: save_location_uri,
                },
            )
            .await
            .map_err(FfiError::from)
    }

    pub async fn reject_receive(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .reject_and_respond(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 取消发送会话。方向由调用方给（RN 侧 projection 带 direction），
    /// 不做「先试发送、失败再试接收」的探测 —— 取消有副作用（发 wire 帧、删半成品、写终态），
    /// 拿它当探针会误触发，错误也只能被拼成两句互相矛盾的话。
    pub async fn cancel_send(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .cancel_send(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 取消接收会话（含清理已落盘的半成品）。
    pub async fn cancel_receive(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .cancel_receive(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 暂停发送会话。
    pub async fn pause_send(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .pause_send(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 暂停接收会话。
    pub async fn pause_receive(&self, session_id: String) -> FfiResult<()> {
        let session_uuid = parse_session_id(&session_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .pause_receive(&session_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 设置全局「暂停接收」。暂停期间节点仍在线可发现、配对不受影响，但对新 offer
    /// 以 ReceivingPaused 婉拒。镜像桌面托盘的「暂停接收」开关（core 3d2d764）。
    pub async fn set_receiving_paused(&self, paused: bool) -> FfiResult<()> {
        let manager = self.transfer_manager_arc().await?;
        manager.set_receiving_paused(paused);
        Ok(())
    }

    /// 查询当前是否暂停接收。节点未启动视为「未暂停」（对齐桌面语义），
    /// 这样 RN 侧在节点未运行时也能安全读取初始状态。
    pub async fn is_receiving_paused(&self) -> FfiResult<bool> {
        match self.transfer_manager_arc().await {
            Ok(manager) => Ok(manager.is_receiving_paused()),
            Err(FfiError::NodeNotStarted) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// 告知内核宿主**此刻**的默认接收落点。
    ///
    /// 设备策略未显式指定落点时，自动接收用它。RN 侧应在节点启动后、以及用户改动接收
    /// 目录之后调用——**这正是它存在的理由**：此前宿主是在设置策略时把当时的落点抄一份
    /// 进每台设备的策略里，用户之后换目录，自动接收还照着旧值写。
    ///
    /// 节点未启动时静默忽略：落点会在下次启动后由 RN 侧重新推上来。
    pub async fn set_default_save_location(&self, uri: String) -> FfiResult<()> {
        match self.transfer_manager_arc().await {
            Ok(manager) => {
                manager.set_host_default_save_location(Some(uri));
                Ok(())
            }
            Err(FfiError::NodeNotStarted) => Ok(()),
            Err(err) => Err(err),
        }
    }
}

fn parse_session_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid session_id: {s}")))
}
