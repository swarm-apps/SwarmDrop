//! 接收目的地和已发布文件位置，云对象不能被解释成本地路径。
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 保存位置
///
/// 桌面端使用文件系统绝对路径。数据库中以 JSON 形式存储在 `save_path` 列，
/// 通过 `FromJsonQueryResult` 自动序列化/反序列化。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SaveLocation {
    /// 桌面端：文件系统绝对路径
    Path { path: String },
    Cloud {
        provider: CloudProvider,
        account_id: String,
        root: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CloudProvider {
    GoogleDrive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CloudObjectRef {
    pub provider: CloudProvider,
    pub account_id: String,
    pub object_id: String,
    pub display_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileLocation {
    Local { uri: String, dir: String },
    Cloud { object: CloudObjectRef },
}
impl FileLocation {
    pub fn local_uri(&self) -> Option<&str> {
        match self {
            Self::Local { uri, .. } => Some(uri),
            Self::Cloud { .. } => None,
        }
    }
    pub fn local_dir(&self) -> Option<&str> {
        match self {
            Self::Local { dir, .. } => Some(dir),
            Self::Cloud { .. } => None,
        }
    }
}
