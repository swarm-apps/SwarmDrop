//! 浏览器连接结果；网络领域读模型沿用共享定义。
use serde::Serialize;

/// 连接路径类别（[`swarmdrop_net_base::PathKind`] 的 JS 投影，TS 侧是字符串联合）。
///
/// `camelCase` 而非 `lowercase`：要与被投影的 `PathKind` 逐字一致，否则双词变体两边
/// 对不上（`holePunched` vs `holepunched`），而单词变体又看不出差别——正是那种在加进
/// 第一个双词变体时才暴露、且没有任何编译错误的错位。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PathKindJson {
    Local,
    Direct,
    HolePunched,
    Relayed,
}

impl From<swarmdrop_net_base::PathKind> for PathKindJson {
    fn from(p: swarmdrop_net_base::PathKind) -> Self {
        match p {
            swarmdrop_net_base::PathKind::Local => Self::Local,
            swarmdrop_net_base::PathKind::Direct => Self::Direct,
            swarmdrop_net_base::PathKind::HolePunched => Self::HolePunched,
            swarmdrop_net_base::PathKind::Relayed => Self::Relayed,
        }
    }
}

/// `connect()` 的结构化返回。
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionJson {
    pub path: PathKindJson,
    pub addr: String,
}
