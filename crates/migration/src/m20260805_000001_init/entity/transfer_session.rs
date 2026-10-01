use super::device::PeerId;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub session_id: Uuid,
    pub direction: TransferDirection,
    #[sea_orm(column_type = "Text")]
    pub peer_id: PeerId,
    pub peer_name: String,
    pub total_size: i64,
    pub transferred_bytes: i64,
    pub status: SessionStatus,
    pub phase: TransferPhase,
    pub suspended_reason: Option<SuspendedReason>,
    pub terminal_reason: Option<TerminalReason>,
    pub epoch: i64,
    pub recoverable: bool,
    pub source_fingerprint: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
    pub origin: Option<String>,
    pub save_path: Option<SaveLocation>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::transfer_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// 传输方向。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
pub enum TransferDirection {
    Send,
    Receive,
}

/// 传输会话状态（旧扁平模型，过渡期保留）。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
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

/// 保存位置（JSON 列）。
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, FromJsonQueryResult,
)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SaveLocation {
    Path { path: String },
}

/// 传输生命周期大状态。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
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

/// suspended 原因。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
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

/// terminal 原因。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
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
    Expired,
}
