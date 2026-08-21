//! 工具的能力端口。
//!
//! ## 这个 trait 存在的唯一理由是「将来搬得走」
//!
//! 桌面壳那 20 个 MCP 工具全部绑在 `tauri::AppHandle` 上（`resolve_transfer(app)`、
//! `mcp_default_receive_dir(app)`、`organization_alias(app, …)`），于是它们只能长在
//! `src-tauri` 里——第二个宿主要用同一批工具时，除了抄一遍没有别的办法。
//!
//! 这里从第一天就把那条线画出来：**工具的 schema、描述与分派只依赖本 trait**，
//! 宿主特有的东西（`DataDir`、clap 的类型、本地通道、退出码）一律留在实现侧。
//! 将来把工具面提到共享 crate 给桌面端一起用时，搬的是文件而不是重写
//! （openspec: `agent-harness-integration` 的 design D7）。
//!
//! ## 为什么取数方法返回 `Value` 而不是强类型
//!
//! 因为本仓的取数路径**天生有两条**，且它们的形态不同：有常驻节点时经本地通道问它
//! （回来的已经是 JSON），没有时直连本机记录（拿到的是强类型）。既有代码把这件事摆在
//! 明处——`cmd::send` 的 `Delivered` 枚举第一个变体就叫「常驻节点做的，回来的已经是
//! JSON」。
//!
//! 于是两个方向的收敛成本完全不对称：收敛到 `Value` 只需在直连那侧调一次 `to_value`；
//! 收敛到强类型则要在通道那侧把 JSON 反序列化回去，而那一步**可能失败**，失败时既没有
//! 好的错误可报、也没有任何人受益——因为 MCP 工具的输出终点本来就是 JSON。强类型在这里
//! 只会换来一次「Value → 强类型 → Value」的空转。
//!
//! 契约因此落在**方法签名**上而不是返回类型上：方法名与参数是这个端口的语义，返回的
//! JSON 结构由 [`swarmdrop_core`] 的投影类型决定（`InboxItemSummary` / `TransferProjection`
//! 等），那些类型本就要过 wasm 门禁，平台中立是它们的硬约束。
//!
//! 例外是 [`ToolBackend::inbox_file_path`]：它的结果是一个要做存在性检查的具体值，
//! 不是一份投影。

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;

use crate::runtime::transfers::Control;

/// 工具执行的失败分类。
///
/// **不复用 [`crate::exit::CliError`]**：那个类型的语义是「一条命令怎么结束」，与退出码
/// 一一对应；而工具调用的失败是一次 RPC 的错误，没有退出码这回事。把两者合并会让这个
/// 端口拖着一整套进程退出的语义走，搬到别的宿主时第一个卡住的就是它。
///
/// 分类刻意粗：MCP 的调用方是模型，它能据以改变行为的只有「我传错了」与「环境不对」
/// 两类。更细的分法（区分 `PeerUnreachable` 与 `TransferFailed` 之类）对退出码有意义，
/// 对模型没有——它拿到的都是一段要读的文字。
#[derive(Debug)]
pub enum ToolError {
    /// 参数不合法、目标解析不了、条目不存在——调用方改参数重来。
    Invalid(String),
    /// 环境不满足：节点不可用、数据库读不到、传输中断。
    Unavailable(String),
}

impl ToolError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }

    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::Unavailable(msg.into())
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(msg) | Self::Unavailable(msg) => f.write_str(msg),
        }
    }
}

pub type ToolResult<T> = Result<T, ToolError>;

/// 工具面需要的全部能力。
///
/// 方法按四类分组，与 spec `cli-mcp-host` 的「工具面」一一对应：发送 / 设备 / 收件箱 /
/// 传输，另加一个节点状态。
///
/// ⚠️ **没有「代收入站传输」**，且这是**故意的**（spec 里是一条 SHALL NOT）。CLI 的常驻
/// 节点对来自已配对设备的入站内容本就自行确认，模型不需要这项能力；把它暴露出去等于把
/// 「收不收」这个决定从人手里移交给模型。要加之前先回去改 spec。
#[async_trait]
pub trait ToolBackend: Send + Sync {
    /// 向一台已配对设备发送文件或目录，阻塞到传输终态。
    async fn send_files(&self, paths: Vec<PathBuf>, to: &str) -> ToolResult<Value>;

    /// 向一台已配对设备发送一段文本，阻塞到拿得出确定结论。
    async fn send_text(&self, body: String, to: &str) -> ToolResult<Value>;

    /// 已配对设备。
    ///
    /// `online_only` 为真时只返回当前在线的那些——「能发给谁」与「配过谁」是两个集合，
    /// 模型问的通常是前者，但排查「为什么发不出去」时要的是后者。
    async fn list_devices(&self, online_only: bool) -> ToolResult<Value>;

    /// 按接收时间倒序列出收件箱条目。
    async fn list_inbox(&self, limit: Option<u32>, include_archived: bool) -> ToolResult<Value>;

    /// 子串检索收件箱。
    ///
    /// ⚠️ 实现**必须**走端口的 `search_inbox_capped` 而不是 `search_inbox`：后者收的是
    /// 确定的 `usize`，于是每个宿主都得自己想「不传时用几」——#111 之前四个宿主想出了
    /// 四个答案，而内核的截断掉的永远是最早收到的那批，表现为同一个词在一端搜得到、
    /// 在另一端搜不到。因此这里的 `limit` 也是 `Option`，让「自带一个默认值」那条路
    /// 在类型上就不存在。
    async fn search_inbox(
        &self,
        query: &str,
        limit: Option<u32>,
        include_archived: bool,
    ) -> ToolResult<Value>;

    /// 一个收件箱条目的详情。
    async fn inbox_item(&self, item_id: &str) -> ToolResult<Value>;

    /// 条目内某个文件在本机的真实路径。
    ///
    /// 文件缺失或路径不可达时返回 [`ToolError::Invalid`]，**不得**返回一个无效路径
    /// （spec: 取收件箱条目的本地路径）——模型拿到无效路径后会把它传给别的工具，
    /// 失败会出现在离原因很远的地方。
    async fn inbox_file_path(&self, item_id: &str, relative_path: &str) -> ToolResult<PathBuf>;

    /// 传输会话，按更新时间倒序。
    async fn list_transfers(&self, limit: Option<u32>) -> ToolResult<Value>;

    /// 单条传输会话的详情。
    async fn transfer_status(&self, session_id: &str) -> ToolResult<Value>;

    /// 对一条会话执行运行控制（暂停 / 恢复 / 取消）。
    ///
    /// [`Control`] 是纯枚举、零平台依赖，因此留在签名里——它是这个端口的**输入**契约，
    /// 而「这个动作此刻对这条会话成不成立」的判据只有 `Control::applies` 一份，
    /// 换成字符串等于在这里开第二份。
    async fn control_transfer(&self, session_id: &str, action: Control) -> ToolResult<Value>;

    /// 节点与网络状态。
    async fn network_status(&self) -> ToolResult<Value>;
}
