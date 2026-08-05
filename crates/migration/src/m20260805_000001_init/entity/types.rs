//! 共享列类型的**冻结快照**，仅供本 migration 建表使用。
//!
//! 逐字复刻 `entity/src/lib.rs` 在本迁移写成时的形态，去掉与建表无关的部分
//! （`specta` 派生、`Display`/`From` 便利 impl、`TransferPhase::legacy_status`）。
//! 时间胶囊的意义就在这份副本上：主 crate 的 entity 之后怎么演进，本迁移建出来的表
//! 都不变。**不要改成 `use entity::...`** —— 那会让这个迁移的行为跟着 entity 漂。

use sea_orm::entity::prelude::*;

/// 设备 `NodeId` 的数据库存储类型（base58btc 字符串）。
#[derive(Clone, Debug, PartialEq, Eq, Hash, DeriveValueType)]
pub struct PeerId(pub String);

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

/// 单文件传输状态。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "lowercase"
)]
pub enum FileStatus {
    Pending,
    Completed,
    Failed,
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

/// 收件箱来源类型。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum InboxSourceKind {
    PairedDevice,
    ShareCode,
    Mcp,
    Unknown,
}

/// 收件箱内容类型。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum InboxContentKind {
    Files,
    Text,
    Clipboard,
    Bundle,
}

/// 邀请状态。
#[derive(Clone, Debug, PartialEq, Eq, DeriveActiveEnum, strum::EnumIter)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    rename_all = "snake_case"
)]
pub enum InviteState {
    Pending,
    Consumed,
    Revoked,
}
