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

    /// 身份存储（密钥材料）的读写**真的失败了**。
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
    /// `initialize_identity` 失败过或还没调用，用户的正确动作是重启应用，而不是去翻身份文件。
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

    /// 指向的会话 / 挂起 offer / 收件箱条目 / 预备传输**不存在**。
    ///
    /// 用户看到的是「这条记录已经不在了」——它已完成、已取消、已被清理，或是另一个窗口
    /// 抢先处理掉了。用户的动作是回列表重来，而不是重试这一次。
    #[error("not found: {0}")]
    SessionNotFound(String),

    /// 落盘失败：写文件、OPFS、sink 写入。
    ///
    /// 与 [`Self::Database`] 的区别是「写用户的数据」与「写我们的账本」：这个用户能处置
    /// （清空间、换保存位置、给权限），那个不能。
    #[error("Storage error: {0}")]
    StorageFailed(String),

    /// 传输域的**其余**失败。
    ///
    /// **它不是垃圾桶，它是「其余」——区别在于有判据。** 新增一种传输失败时先问：
    /// **UI 能据此给出与其他 kind 不同的、用户真能照做的建议吗？**
    ///
    /// 能 → 拆一个 kind 出去（已拆的两个：[`Self::SessionNotFound`]「回列表重来」、
    /// [`Self::StorageFailed`]「清空间或换位置」）。不能 → 留在这里。锁中毒、JS 句柄类型
    /// 异常、range 溢出、序列化失败、协议帧不合法都属于后者：它们对用户是同一件事
    /// 「出了个你处理不了的问题」，各造一个 kind 只会让三端文案表膨胀，而每条文案都只能
    /// 写成「出错了，请重试」的同义句。
    ///
    /// **判据还有第二问：这个 kind 真的到得了 UI 吗？** 内容校验失败（bao 逐块验签、
    /// checksum 比对）看着完全够格 —— 用户动作明确且唯一，就是重传一次。但它只发生在
    /// ReceiverActor 里，那条路径的失败走 `ActorReport::FatalError(String)` → 落库
    /// `error_message` → 详情页渲染那个 String，**根本不经过 `kind`**。给它造一个 kind
    /// 等于造一个永远不会被任何文案表命中的判别码。要修的是那条 String 通道，不是这里。
    ///
    /// 反面教材是 [`Self::Identity`]：它当年的问题不是承载得多，是**没有判据**，
    /// 于是 peer_id 解析失败也往里塞。
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
            AppError::SessionNotFound(msg) => ("SessionNotFound", msg.clone()),
            AppError::StorageFailed(msg) => ("StorageFailed", msg.clone()),
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
