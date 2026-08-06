//! 传输控制面请求/响应类型（wire v2）。
//!
//! wire v2 删除了应用层 XChaCha20 加密：Noise/TLS 在途已加密，relay 只见密文，
//! 密钥经同一加密信道分发是自引用——故 `OfferResult` / `ResumeCommit` 不再携带
//! 传输密钥，数据面直接传明文（帧协议见 [`transfer::wire`](crate::wire)）。

use serde::{Deserialize, Serialize};
use swarmdrop_net::{ProtocolId, Rpc};
use uuid::Uuid;

use entity::TerminalReason;

/// 传输控制面协议名（typed RPC）。
pub const TRANSFER_CTRL_PROTOCOL: ProtocolId =
    ProtocolId::from_static("/swarmdrop/transfer-ctrl/2");

/// 传输数据面协议名（裸流 + 自带帧协议，见 [`wire`](crate::wire)）。
///
/// **v3 = v2 + 流控窗口帧**（[`TransferDataFrame::Window`](crate::wire::data_frame::TransferDataFrame::Window)）。
///
/// 加一个帧 tag 就必须换协议名，因为解码器对未知 tag 是**硬失败**：v2 的
/// `decode_frame` 遇到 tag 7 返回协议错误，接收端随即中止整个会话。若沿用 `/2`，
/// v0.12.1 的发送端会在第一个窗口边界（4 MiB）静默打断每一个 v0.12.0 接收端——
/// 而移动端的更新永远滞后于桌面端，那正是「桌面发照片到手机」这条最常走的路径。
pub const TRANSFER_DATA_PROTOCOL: ProtocolId =
    ProtocolId::from_static("/swarmdrop/transfer-data/3");

/// 上一版数据面协议，**仍在服务**（v0.12.0 及更早只认它）。
///
/// 两个方向各要它一次：旧发送端只会拨 `/2`，所以接收侧必须继续注册；新发送端拨 `/3`
/// 被拒时退回这里（见 `wire::data_plane` 的 `open_data_stream`），退回后整条链路不发
/// 窗口帧。
///
/// 删除它的条件：不再需要兼容 v0.12.0——即 SwarmHive 上桌面与移动的 stable 都已越过
/// 那一版足够久。**不要凭桌面端单独判断**，移动端是滞后的那一端。
pub const TRANSFER_DATA_PROTOCOL_V2: ProtocolId =
    ProtocolId::from_static("/swarmdrop/transfer-data/2");

/// 传输控制面 typed RPC：`TransferRequest → TransferResponse`。
pub const TRANSFER_CTRL: Rpc<TransferRequest, TransferResponse> = Rpc::new(TRANSFER_CTRL_PROTOCOL);

/// 传输文件元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_id: u32,
    pub name: String,
    /// 对端声明的相对路径，接收方据此在保存目录下建文件。
    ///
    /// **这是对端完全可控的字符串，落盘前必须过 [`is_safe_relative_path`]。**
    pub relative_path: String,
    pub size: u64,
    pub checksum: String,
}

/// 一条对端声明的 `relative_path` 能否安全地拼到本机保存目录下。
///
/// # 为什么必须有这道校验
///
/// 接收侧最终做的是 `save_dir.join(relative_path)`（桌面
/// `src-tauri/src/host/file_sink/path_ops.rs`，Web 是 OPFS 的目录链）。Rust 的
/// [`Path::join`](std::path::Path::join) 遇到**绝对路径会把 base 整段丢弃**，而 `..`
/// 会向上穿越；`resolve_paths` 还会 `create_dir_all(parent)` 把目标目录建出来。所以
/// 一条 `/etc/cron.d/evil` 或 `../../../../.ssh/authorized_keys` 等于让已配对的对端
/// **在本机任意位置写文件**。
///
/// 配对不蕴含这个权限：产品自己就定义了 `temporary` / `collaborator` 这些低于 `owned`
/// 的信任级别，而它们与 `owned` 在这条路径上此前没有任何区别。
///
/// # 判据是纯字符串的，故意不用 [`std::path`]
///
/// `std::path` 的语义随目标平台变：`..\..\x` 在 Unix 上是**一个**普通文件名，在 Windows 上
/// 是两级穿越。而收到这条 offer 的可能是任意一端，判据必须三端逐字一致——否则同一条恶意
/// 路径在 Linux 上被放行、到了 Windows 才生效。所以两种分隔符都当分隔符，盘符前缀一并拒。
///
/// # 「只由点和空白组成的段」整族拒绝，不是只拒 `.` 与 `..`
///
/// 这条是**纵深防御，不是在补一个已知可利用的洞**——区别很重要，别当成可以放松的冗余。
///
/// 起因是 Win32 会裁剪路径里的空格与句点，看上去 `".. "` 应当等价于 `".."`。查证后
/// （见文末出处）实际有两条**互不相同**的规则：
/// - **逐段**：段尾若是*恰好一个*句点则删除。三个及以上句点的段是合法文件名，不动。
/// - **仅整条路径末尾**：末尾的句点与空格全删——**只作用于最后一段**。
///
/// 于是 `".. /evil.txt"` 穿不过去：`.. ` 是中间段，两条规则都不命中，保持字面目录名。
/// 它今天不可利用，靠的是这两条规则恰好不一致。
///
/// **我们不打算依赖这个巧合。** 判据因此是语义的：一个段若去掉点和空白就什么都不剩，
/// 它就不是文件名，只可能是相对路径记号或某种规范化的产物。合法发送端从不产生这种段，
/// 拒绝它零代价，而且省掉了「每次改动都要重新推演一遍 Win32 规范化」这件事。
///
/// ⚠️ **绝不要改成「先 trim 再比对 `..`」**。那会把 `".. "` 真的归一成 `".."`——若归一化的
/// 结果又被拿去用（而不是只用于判定），就等于亲手造出这个洞。这里只做**判定**，从不改写路径。
///
/// 已知不拒、留给宿主的两类（都**不构成逃逸**，写入仍落在保存目录内）：
/// - Windows 保留设备名（`CON` / `NUL` / `COM1`…）与段内冒号（NTFS 备用数据流）——
///   前者写进设备、后者挂成隐藏流，那一条传输自己失败或落得古怪，但出不了保存目录。
/// - 尾部带点或空格的**真实**文件名（`"foo. "`）——Windows 规范化成 `"foo"`，可能与同批的
///   `"foo"` 撞名。为它整条拒绝会误伤 Unix 上合法的文件名，代价不对称。
///
/// # 词法检查看不见文件系统
///
/// 本函数**只保证这个字符串本身不含逃逸记号**。「拼出来的路径最终落在保存目录内」还需要
/// 宿主侧的一次实际包含性检查——保存目录下若存在指向外部的符号链接，一条完全合法的
/// `sub/x.txt` 照样写得出去。那道兜底在
/// `src-tauri/src/host/file_sink/path_ops.rs::resolve_paths`。
///
/// 出处：Microsoft「Path Normalization」/「File path formats on Windows systems」，
/// 以及 Project Zero「The Definitive Guide on Win32 to NT Path Conversion」。
///
/// # 拒绝而不是「修正」
///
/// 静默归一化会把攻击变成一次「文件莫名其妙落在别处」的怪事，而合法发送端**永远**不会
/// 产生 `..`、绝对路径或盘符——浏览器的 `webkitRelativePath` 与桌面的枚举器都不会。
/// 所以不安全就整条 offer 拒掉，让它响亮地失败。
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') {
        return false;
    }
    // 绝对路径（POSIX `/x`、Windows `\x` 与 UNC `\\host\share`）。
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    // Windows 盘符：`C:\x` 是绝对路径，`C:x` 是「C 盘的当前目录」——两者都不该出现。
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return false;
    }
    path.split(['/', '\\']).all(is_safe_segment)
}

/// 单个路径段是否是**真实的名字**（而非相对路径记号或其变体）。见上方文档的第二节。
fn is_safe_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.chars().all(|c| c == '.' || c.is_whitespace())
}

/// 一条 [`FileInfo`] 的 `name` 与 `relative_path` 是否自洽（`name` 必须是路径的末段）。
///
/// # 为什么这也是安全检查
///
/// 两个字段都由对端**完全控制**，此前无任何一致性约束。而接收端的两个用途是分开的：
/// 确认弹窗与文件列表展示 `name`，落盘只用 `relative_path`。于是
/// `name: "photo.jpg"` + `relative_path: "autostart/evil.desktop"` 让用户在决定是否接收时
/// 看到的是一个无害的图片，落到磁盘上的却是一个自启动项——路径本身完全合法，逃不出保存
/// 目录，但**用户据以决策的信息是假的**。
///
/// 收在这里而不是让每个前端改成显示 basename：那样三端要各改一次，且下一个新增的展示点
/// 又会漏。合法发送端天然满足这条（三端的 `name` 都是从路径末段取的）。
pub fn file_info_is_consistent(info: &FileInfo) -> bool {
    info.relative_path
        .rsplit(['/', '\\'])
        .next()
        .is_some_and(|basename| basename == info.name)
}

/// 传输发起来源：人在应用内发起，或 AI 代理经 MCP 发起。
///
/// 由发送方自报、承载于 Offer，供接收端展示与 inbox 来源派生——是信息性/UX 信号，
/// 不作接收端安全边界（真正的控制是发送端 `allow_mcp_send_to_device` 门控）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TransferOrigin {
    /// 用户在应用内手动发起。
    Human,
    /// AI 代理经 MCP 发起；`client` 为 MCP 客户端名（如 claude-desktop），不可得时为 None。
    Mcp { client: Option<String> },
}

impl TransferOrigin {
    /// 序列化为 DB 列存储的紧凑字符串：`human` / `mcp` / `mcp:<client>`。
    pub fn to_db_string(&self) -> String {
        match self {
            TransferOrigin::Human => "human".to_string(),
            TransferOrigin::Mcp { client: Some(c) } => format!("mcp:{c}"),
            TransferOrigin::Mcp { client: None } => "mcp".to_string(),
        }
    }

    /// 从 DB 列字符串解析；无法识别（含历史 NULL→`"human"`）时回退 `Human`。
    pub fn from_db_string(s: &str) -> Self {
        match s {
            "mcp" => TransferOrigin::Mcp { client: None },
            other => match other.strip_prefix("mcp:") {
                Some(client) => TransferOrigin::Mcp {
                    client: Some(client.to_string()),
                },
                None => TransferOrigin::Human,
            },
        }
    }

    /// 是否为 MCP / AI 代理来源。
    pub fn is_mcp(&self) -> bool {
        matches!(self, TransferOrigin::Mcp { .. })
    }
}

/// 断点续传被拒绝的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResumeRejectReason {
    Cancelled,
    FatalError,
    SourceModified,
    CheckpointInvalid,
    PeerUnavailable,
    SessionNotFound,
}

/// 待传 byte range（fetch_plan 元素，恢复探测协议用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRange {
    pub file_id: u32,
    pub offset: u64,
    pub length: u64,
}

/// 单文件 checkpoint（已完成 byte range 列表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCheckpoint {
    pub file_id: u32,
    pub completed_ranges: Vec<(u64, u64)>,
}

/// 恢复探测时对端报告的 phase（简化映射）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResumePhaseReport {
    NotFound,
    Active,
    Suspended,
    Terminal,
}

/// 恢复状态报告内容（ResumeProbe 的应答体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeReport {
    pub phase: ResumePhaseReport,
    pub epoch: i64,
    pub files: Vec<FileInfo>,
    pub checkpoint: Vec<FileCheckpoint>,
    pub source_fingerprint: Option<String>,
    pub terminal: bool,
    pub terminal_reason: Option<TerminalReason>,
}

/// 传输请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TransferRequest {
    Offer {
        session_id: Uuid,
        files: Vec<FileInfo>,
        total_size: u64,
        /// 发起来源（人工 / MCP 代理），由发送方自报。
        origin: TransferOrigin,
    },
    Cancel {
        session_id: Uuid,
        reason: String,
    },
    Pause {
        session_id: Uuid,
    },
    /// 恢复探测：发起方询问对端会话当前事实。
    ResumeProbe {
        session_id: Uuid,
    },
    /// 恢复提交：发起方确认恢复，携带新 epoch 和 fetch_plan。
    ResumeCommit {
        session_id: Uuid,
        new_epoch: i64,
        fetch_plan: Vec<FileRange>,
    },
}

/// Offer 被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OfferRejectReason {
    NotPaired,
    UserDeclined,
    PolicyRejected,
    /// 接收方处于全局「暂停接收」状态，婉拒新 offer。
    ReceivingPaused,
    /// Offer 里有文件的 `relative_path` 会逃出保存目录（绝对路径、盘符或 `..` 穿越）。
    ///
    /// 独立成一个变体而不是并进 `PolicyRejected`：这**不是**接收方的偏好设置拒绝了你，
    /// 而是这条 offer 本身不合法。两者对发送方的含义完全不同——前者「换个设置或问问对方」，
    /// 后者「你的客户端发了非法数据」。合法客户端永远不会触发它。
    UnsafePath,
}

/// 传输响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TransferResponse {
    OfferResult {
        accepted: bool,
        reason: Option<OfferRejectReason>,
    },
    Ack {
        session_id: Uuid,
    },
    /// 恢复状态报告（对 ResumeProbe 的应答）。
    ResumeStateReport {
        session_id: Uuid,
        report: ResumeReport,
    },
    /// 恢复确认（对 ResumeCommit 的应答）。
    ResumeAck {
        session_id: Uuid,
        new_epoch: i64,
        accepted: bool,
        reason: Option<ResumeRejectReason>,
    },
}

#[cfg(test)]
mod tests {
    use super::{FileInfo, TransferOrigin, file_info_is_consistent, is_safe_relative_path};

    #[test]
    fn transfer_origin_db_string_roundtrip() {
        for origin in [
            TransferOrigin::Human,
            TransferOrigin::Mcp { client: None },
            TransferOrigin::Mcp {
                client: Some("claude-desktop".to_string()),
            },
        ] {
            let s = origin.to_db_string();
            assert_eq!(
                TransferOrigin::from_db_string(&s),
                origin,
                "roundtrip via {s}"
            );
        }
    }

    #[test]
    fn transfer_origin_from_db_string_fallback() {
        assert_eq!(
            TransferOrigin::from_db_string("human"),
            TransferOrigin::Human
        );
        // 未知 / 历史 NULL→"" 一律回退 Human，绝不 panic。
        assert_eq!(
            TransferOrigin::from_db_string("unknown"),
            TransferOrigin::Human
        );
        assert_eq!(TransferOrigin::from_db_string(""), TransferOrigin::Human);
        assert_eq!(
            TransferOrigin::from_db_string("mcp"),
            TransferOrigin::Mcp { client: None }
        );
        assert_eq!(
            TransferOrigin::from_db_string("mcp:cursor"),
            TransferOrigin::Mcp {
                client: Some("cursor".to_string())
            }
        );
    }

    /// 合法发送端产生的路径必须全部放行——否则这道校验会变成「正常传输随机失败」。
    #[test]
    fn ordinary_relative_paths_are_accepted() {
        for ok in [
            "a.txt",
            "photos/2026/a.jpg",
            "带空格 和中文/文件.pdf",
            "a..b.txt",  // `..` 只在**整段**是它时才算穿越
            "...hidden", // 前导点不是穿越
            "x/..y/z",
            "C.txt", // 单字母 + 点，不是盘符
        ] {
            assert!(is_safe_relative_path(ok), "应当放行: {ok:?}");
        }
    }

    /// 这条判据松掉一格就是**已配对对端的任意文件写**（见 `is_safe_relative_path` 文档）。
    ///
    /// Windows 分隔符与盘符单独列出来是刻意的：判据是纯字符串的，因为收到这条 offer 的
    /// 可能是任意一端，不能出现「Linux 放行、Windows 才生效」的错位。
    #[test]
    fn escaping_relative_paths_are_rejected() {
        for bad in [
            "",
            "/etc/cron.d/evil",
            "//etc/passwd",
            "\\Windows\\System32\\x.dll",
            "\\\\host\\share\\x", // UNC
            "C:\\Windows\\x.dll",
            "c:x.txt", // 盘符相对路径
            "../x",
            "..",
            "a/../../b",
            "a\\..\\..\\b", // Windows 分隔符下的穿越
            "./a",
            "a//b", // 空段
            "a/\0b",
        ] {
            assert!(!is_safe_relative_path(bad), "应当拒绝: {bad:?}");
        }
    }

    /// 纵深防御：只由点与空白组成的段一律拒。
    ///
    /// 这些今天在 Windows 上**不可利用**（逐段规则只删句点、删空格那条只作用于整条路径的
    /// 最后一段，两者不一致恰好挡住了它）。拒绝它们是为了不依赖那个巧合——见
    /// [`is_safe_relative_path`] 的文档。
    #[test]
    fn dot_and_space_only_segments_are_rejected() {
        for bad in [".. /x", "a/.. /b", ".../x", ". ./x", " /x", "a/ /b", "..."] {
            assert!(!is_safe_relative_path(bad), "应当拒绝: {bad:?}");
        }
        // 但**含有**点/空格的真实名字不受影响。
        for ok in ["a. b.txt", " leading.txt", "trailing .txt", "...hidden"] {
            assert!(is_safe_relative_path(ok), "应当放行: {ok:?}");
        }
    }

    fn info(name: &str, relative_path: &str) -> FileInfo {
        FileInfo {
            file_id: 0,
            name: name.into(),
            relative_path: relative_path.into(),
            size: 0,
            checksum: String::new(),
        }
    }

    /// `name` 与 `relative_path` 必须自洽——否则用户在确认弹窗看到的东西和落盘的不是一个。
    #[test]
    fn file_name_must_be_the_path_basename() {
        assert!(file_info_is_consistent(&info("a.txt", "a.txt")));
        assert!(file_info_is_consistent(&info("a.txt", "dir/a.txt")));
        assert!(file_info_is_consistent(&info("a.txt", "dir\\a.txt")));

        // 展示一个无害的名字、落盘另一个东西：正是这条要挡的。
        assert!(!file_info_is_consistent(&info(
            "photo.jpg",
            "autostart/evil.desktop"
        )));
        // 名字是路径的**前**段而非末段，同样不算自洽。
        assert!(!file_info_is_consistent(&info("dir", "dir/a.txt")));
        assert!(!file_info_is_consistent(&info("", "a.txt")));
    }
}
