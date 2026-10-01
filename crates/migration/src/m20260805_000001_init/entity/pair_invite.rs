use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "pair_invites")]
pub struct Model {
    /// `sha256(capability)` 的小写 hex。表里没有 capability 明文，也没有邀请全串。
    #[sea_orm(primary_key, auto_increment = false)]
    pub capability_hash: String,
    pub inviter_id: super::device::PeerId,
    #[sea_orm(indexed)]
    pub expires_at: i64,
    pub state: InviteState,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}

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
