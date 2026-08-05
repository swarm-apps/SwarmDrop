//! 平台无关错误类型。

use serde::Serialize;
use thiserror::Error;

/// Core 层统一错误类型。
#[derive(Debug, Error)]
pub enum AppError {
    /// 文件系统错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 序列化/反序列化错误
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// P2P 网络错误
    #[error("Network error: {0}")]
    Network(String),

    /// 身份存储（keychain / 密钥材料）的读写**真的失败了**。
    ///
    /// **不要拿它当配对路径的垃圾桶。** 它一度承载了 8 处毫不相关的失败——peer_id 解析、
    /// multiaddr 解析、二维码生成、邀请标识格式、邀请状态没落盘、设备找不到——而前端按
    /// `kind` 渲染文案，于是用户在「点接受配对」时会看到一句「设备身份初始化失败」。
    /// 那条提示与真实原因毫无关系，把排查引向完全错误的方向。新增失败模式时先问一句：
    /// **这真的是密钥材料的问题吗？**
    #[error("Identity error: {0}")]
    Identity(String),

    /// 设备身份尚未就绪（私钥还没加载进内存）。
    ///
    /// 与 [`Self::Identity`] 的区别是「没走到」与「做了但失败」：这个通常意味着启动时的
    /// `initialize_identity` 失败过或还没调用，用户的正确动作是重启应用，而不是排查钥匙串。
    #[error("identity not ready")]
    IdentityNotReady,

    /// 调用方传入的参数不合法：peer_id / multiaddr / 邀请标识等的解析失败。
    ///
    /// 这类错误**用户无能为力也看不懂**——要么是 UI 传错了值（bug），要么是内部标识格式
    /// 不对。前端不为它单独渲染文案，走通用兜底；技术细节只进日志。
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// 一次性邀请的「已消费」状态没能落盘，本次配对**已中止**。
    ///
    /// **不是 [`Self::InvalidCode`]**：凭证本身是好的，是本机没写成库，所以宁可让这次配对
    /// 失败也不放行——否则重启后同一份一次性凭证还能再被消费一次。两者的用户动作也不同：
    /// 这个要重新生成一条邀请，那个要换一条来源。
    #[error("invite state not persisted")]
    InvitePersistFailed,

    /// 找不到指定的已配对设备。
    #[error("paired device not found")]
    DeviceNotFound,

    /// 节点未启动
    #[error("Node not started")]
    NodeNotStarted,

    /// 邀请已过期。
    ///
    /// 变体名沿用「Code」是历史（6 位配对码时代），现在承载的是 PairInvite 的过期
    /// ——**不要照名字去理解语义**。改名要动 core / uniffi / TS bindings / 前端
    /// `KIND_MESSAGES` 四处的 `kind` 契约，与本次改动无关，故只在此说明。
    ///
    /// 面向用户的文案由前端按 `kind` 渲染，此处仅作语言无关技术描述。
    #[error("invite expired")]
    ExpiredCode,

    /// 邀请无效：解析 / 验签失败，或凭证已被消费、已撤销。
    ///
    /// 变体名同上，是历史遗留。技术细节（哪一步失败）只进日志 —— 它对用户没有意义，
    /// 而且是 Rust 侧的中文串，直接展示会在非中文界面露馅。
    #[error("invalid invite")]
    InvalidCode,

    /// tokio 任务错误
    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    /// 文件传输错误
    #[error("Transfer error: {0}")]
    Transfer(String),

    /// 数据库错误
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

/// 统一序列化为 `{ kind, message }`，便于各 host 投影到前端错误。
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("AppError", 2)?;

        let (kind, message) = match self {
            AppError::Io(e) => ("Io", e.to_string()),
            AppError::Serialization(e) => ("Serialization", e.to_string()),
            AppError::Network(msg) => ("Network", msg.clone()),
            AppError::Identity(msg) => ("Identity", msg.clone()),
            AppError::IdentityNotReady => ("IdentityNotReady", self.to_string()),
            AppError::InvalidArgument(msg) => ("InvalidArgument", msg.clone()),
            AppError::InvitePersistFailed => ("InvitePersistFailed", self.to_string()),
            AppError::DeviceNotFound => ("DeviceNotFound", self.to_string()),
            AppError::NodeNotStarted => ("NodeNotStarted", self.to_string()),
            AppError::ExpiredCode => ("ExpiredCode", self.to_string()),
            AppError::InvalidCode => ("InvalidCode", self.to_string()),
            AppError::TaskJoin(e) => ("TaskJoin", e.to_string()),
            AppError::Transfer(msg) => ("Transfer", msg.clone()),
            AppError::Database(e) => ("Database", e.to_string()),
        };

        state.serialize_field("kind", kind)?;
        state.serialize_field("message", &message)?;
        state.end()
    }
}

/// Result 类型别名。
pub type AppResult<T> = Result<T, AppError>;
