use sea_orm::entity::prelude::*;

/// 本机发出的配对邀请（一次性凭证的状态表，openspec: invite-persistence）。
///
/// **只存 `sha256(capability)`，不存 capability 明文、也不存邀请全串** —— 库被读到也拿不到
/// 能用的凭证。代价是重启后邀请列表只能显示元数据（创建时间 / 有效期 / 状态），
/// **显示不出原始链接**；想再分享就生成新的并撤销旧的。
///
/// 这张表是内存注册表的**写穿备份**，不是权威判定点：一次性消费的 CAS 仍在
/// `swarmdrop_invite::InviteRegistry` 的单锁内完成，落盘是其后置动作
/// （理由见 openspec/changes/invite-persistence/design.md D2）。
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "pair_invites")]
pub struct Model {
    /// `sha256(capability)` 的小写 hex。主键即查找键 —— 消费与撤销都按它定位。
    #[sea_orm(primary_key, auto_increment = false)]
    pub capability_hash: String,
    /// 发起方身份（当前恒为本机；留字段是为了将来多身份场景不必改表）。
    pub inviter_id: super::PeerId,
    /// 过期时刻（Unix 秒）。清理与 TTL 判定都读它，故建了索引。
    pub expires_at: i64,
    pub state: InviteState,
    /// 创建时刻（Unix 秒）。邀请列表按它倒序。
    pub created_at: i64,
}

/// 邀请状态（三态与内存态一一对应）。
///
/// **`Revoked` 独立于 `Consumed` 是 UX 需要**：撤销与「被对方用掉」在列表里要能区分，
/// 否则用户自己撤销的那条会显示成「已被对方使用」。它**不是**安全措施 ——
/// 写穿失败时写状态与删行的后果相同（库里都留下 `Pending`），详见
/// `swarmdrop_invite::store::PersistedInviteState` 的文档。
///
/// 终态保留到过期之后才清，同样是为了让发起方看到这条的去向，而不是让它凭空消失
/// （**UX 理由，不是安全理由**）：安全侧 fail-closed，注册表查不到即
/// `InviteRejectReason::Unknown` 拒绝。
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    DeriveActiveEnum,
    strum::EnumIter,
)]
#[serde(rename_all = "snake_case")]
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

impl ActiveModelBehavior for ActiveModel {}
