use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PeerId;

/// 文本投递账本。正文只在本表保存；收件箱条目仅以 delivery_id 引用它。
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "text_deliveries")]
pub struct Model {
    /// 发起方生成的稳定投递标识，也是重试幂等键。
    #[sea_orm(primary_key, auto_increment = false)]
    pub delivery_id: Uuid,
    pub direction: TextDeliveryDirection,
    #[sea_orm(column_type = "Text")]
    pub peer_id: PeerId,
    pub peer_name: String,
    pub body: String,
    pub status: TextDeliveryStatus,
    pub failure: Option<TextDeliveryFailure>,
    /// 相同 delivery_id 的显式发送/重试次数。
    pub attempt_count: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}

/// 文本投递账本的方向。
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
pub enum TextDeliveryDirection {
    Send,
    Receive,
}

/// 文本投递的用户可见状态。
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
pub enum TextDeliveryStatus {
    Sending,
    WaitingConfirmation,
    Delivered,
    Rejected,
    Retryable,
    Expired,
    Cancelled,
}

/// 可安全展示给发起方的文本投递失败分类。
///
/// 这里刻意不记录接收端的策略细节，避免把对方的信任与暂停状态泄露到网络边界之外。
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
pub enum TextDeliveryFailure {
    PeerUnavailable,
    TimedOut,
    UnsupportedProtocol,
    Rejected,
    Expired,
    StorageFailed,
    ProtocolConflict,
    InvalidPayload,
}
