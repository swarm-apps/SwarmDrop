//! 持久化设备标识；用于会话、收件箱及邀请关联。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 设备 NodeId 的数据库存储类型，以 base58btc 字符串持久化。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, DeriveValueType)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PeerId(pub String);

impl PeerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PeerId {
    fn from(s: &str) -> Self {
        PeerId(s.to_owned())
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
