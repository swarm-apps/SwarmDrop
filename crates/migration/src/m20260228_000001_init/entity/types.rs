use sea_orm::entity::prelude::*;

/// libp2p PeerId — 冻结快照，仅供 migration 建表使用。
#[derive(Clone, Debug, PartialEq, Eq, Hash, DeriveValueType)]
pub struct PeerId(pub String);

/// 传输方向
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

/// 传输会话状态
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

/// 单文件传输状态
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
