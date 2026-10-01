//! 初始迁移使用的冻结设备标识，不跟随当前领域模型演进。
use sea_orm::entity::prelude::*;

/// 设备 `NodeId` 的数据库存储类型（base58btc 字符串）。
#[derive(Clone, Debug, PartialEq, Eq, Hash, DeriveValueType)]
pub struct PeerId(pub String);
