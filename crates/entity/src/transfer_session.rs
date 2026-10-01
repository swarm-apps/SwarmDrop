use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{PeerId, SaveLocation};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_sessions")]
pub struct Model {
    /// 会话 ID（UUID），来自协议层，收发双方各自独立记录
    #[sea_orm(primary_key, auto_increment = false)]
    pub session_id: Uuid,
    /// 传输方向
    pub direction: TransferDirection,
    /// 对端 libp2p PeerId（base58btc 字符串表示）
    #[sea_orm(column_type = "Text")]
    pub peer_id: PeerId,
    /// 对端设备名（快照，不跟踪更新）
    pub peer_name: String,
    /// 所有文件总字节数
    pub total_size: i64,
    /// 已传输字节数（实时更新）
    pub transferred_bytes: i64,
    /// 会话状态（旧扁平模型，过渡期保留，逐步由 phase + reason 替代）
    pub status: SessionStatus,
    /// 生命周期大状态
    pub phase: TransferPhase,
    /// suspended 原因（phase=suspended 时有值）
    pub suspended_reason: Option<SuspendedReason>,
    /// terminal 原因（phase=terminal 时有值）
    pub terminal_reason: Option<TerminalReason>,
    /// 当前 epoch（每次开始 / 恢复递增，防旧消息污染）
    pub epoch: i64,
    /// 是否可恢复
    pub recoverable: bool,
    /// 源文件指纹（恢复校验用，JSON 编码）
    pub source_fingerprint: Option<String>,
    /// 开始时间（Unix ms）
    pub started_at: i64,
    /// 最后更新时间（Unix ms），用于 paused 会话 7 天过期清理
    pub updated_at: i64,
    /// 完成/失败/取消时间（Unix ms），进行中为 NULL
    pub finished_at: Option<i64>,
    /// 失败原因（status=failed 时有值）
    pub error_message: Option<String>,
    /// 入站 Offer 的接收策略动作快照：auto_accept / require_confirmation / reject。
    pub policy_action: Option<String>,
    /// 入站 Offer 的接收策略原因快照，用于活动与恢复页解释自动接收或拒绝。
    pub policy_reason: Option<String>,
    /// 传输发起来源（紧凑字符串：human / mcp / mcp:<client>），收发双方各自记录。
    pub origin: Option<String>,
    /// 接收方保存位置（direction=receive 时有值）
    /// JSON 序列化的 SaveLocation 枚举
    pub save_path: Option<SaveLocation>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::transfer_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// 传输方向
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
pub enum TransferDirection {
    Send,
    Receive,
}

/// 传输会话状态
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
pub enum SessionStatus {
    Transferring,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// 传输生命周期大状态（phase）。
/// 替代旧的扁平 [`SessionStatus`]（过渡期并存）：phase 表达大状态，
/// 具体原因由 [`SuspendedReason`] / [`TerminalReason`] 表达。
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum TransferPhase {
    Offered,
    WaitingAccept,
    Active,
    Suspended,
    Terminal,
}

/// suspended 原因（phase=Suspended 时有值）。
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum SuspendedReason {
    LocalPaused,
    RemotePaused,
    Interrupted,
    PeerOffline,
    AppRestarted,
}

/// terminal 原因（phase=Terminal 时有值）。
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveActiveEnum, strum::EnumIter,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum TerminalReason {
    Completed,
    Cancelled,
    Rejected,
    FatalError,
    /// 入站 offer 的决策窗口耗尽，本端从未作答。
    ///
    /// **与 `Rejected` 分开是必要的，不是措辞讲究。** 对端看到的确实是一次婉拒（清理任务
    /// drop 掉 responder，RPC handler 据此回复），但本端用户**什么都没做**——把它记成
    /// 「已拒绝」等于在他自己的传输历史里写一条他没做过的决定，而这恰恰是他下次想不起来
    /// 「我拒过这个人吗」时会去查的地方。
    Expired,
}

impl TransferPhase {
    /// 过渡期桥接：把新 phase + reason 映射回旧扁平 [`SessionStatus`]。
    ///
    /// 前端旧路径与未迁移代码仍读 `status` 列，Coordinator 写 phase 时必须经此
    /// 同步 `status`，避免两种表示漂移（单一映射来源）。迁移完成后随 `SessionStatus`
    /// 一并移除。
    pub fn legacy_status(&self, terminal_reason: Option<&TerminalReason>) -> SessionStatus {
        match self {
            TransferPhase::Offered | TransferPhase::WaitingAccept | TransferPhase::Active => {
                SessionStatus::Transferring
            }
            TransferPhase::Suspended => SessionStatus::Paused,
            TransferPhase::Terminal => match terminal_reason {
                Some(TerminalReason::Completed) => SessionStatus::Completed,
                // Expired 归 Cancelled：旧扁平枚举没有「没答复」这一档，而它离
                // 「没传成，但也不是错误」最近。新路径读 terminal_reason，不受这个粗粒度影响。
                Some(TerminalReason::Cancelled)
                | Some(TerminalReason::Rejected)
                | Some(TerminalReason::Expired) => SessionStatus::Cancelled,
                Some(TerminalReason::FatalError) | None => SessionStatus::Failed,
            },
        }
    }
}
