use sea_orm::entity::prelude::*;

use crate::{PeerId, TextDeliveryDirection, TextDeliveryFailure, TextDeliveryStatus};

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
