//! 会话失败原因的**判别码**。
//!
//! 这是 `AppError::kind` 那条契约在**会话级失败**上的延续：Rust 只回答「是什么失败」，
//! 用户看到的句子由三端各自的 Lingui catalog 生成。
//!
//! 它替换的是一段自由文本。旧形态
//! `format!("文件最终化失败: {name} (file_id={id}): {e}")` 有两个问题，第二个是真 bug：
//!
//! 1. 中文串直达三端 UI，英文界面上照样弹中文。
//! 2. **文件名被拼进了错误串**，而移动端拿整串跑英文关键词正则 —— 一个叫
//!    `Q3-cancel.xlsx` 的文件校验失败时，用户看到的是「传输已取消」，
//!    一次数据损坏被说成用户自己的操作。
//!
//! 所以判别码的参数**只放渲染真正需要的字段**：文件名要显示，故保留；`file_id`
//! 与底层错误只对开发者有意义，留在 `warn!` 里，不进用户可见的数据结构。

use serde::{Deserialize, Serialize};

use crate::protocol::ResumeRejectReason;

/// 会话失败原因。持久化进 `transfer_sessions.error_message` 列（类型不变，存 JSON）。
///
/// 变体数量刻意贴着**实际构造点**（三处 `ActorReport::FatalError` + 一处过期回收），
/// 不预留「将来可能用到」的码 —— `failure-semantics-contract` 的 D3 已经吃过一次亏：
/// 造出来到不了 UI 的判别码只是三端文案表里的死条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum FailureCode {
    /// 落盘最终化失败 —— 含 bao 逐块验签不通过、sink 写入失败。
    ///
    /// 用户能做的是重新传一次，所以三端文案落在「文件没能完整保存，请重新接收」。
    #[serde(rename_all = "camelCase")]
    FileFinalizeFailed { file_name: String },

    /// 超过保留期仍未恢复，被启动清理回收。
    ///
    /// `retention_days` 进文案（「超过 N 天」），所以它是参数而不是常量 ——
    /// 保留期是配置项，两端可能不同。
    #[serde(rename_all = "camelCase")]
    SessionExpired { retention_days: u32 },

    /// 对端拒绝了续传请求。
    ///
    /// **直接内嵌 [`ResumeRejectReason`]，不再压成字符串。** 这条通道此前经
    /// `resume_reject_message()` 把一个六变体的枚举摊平成六句中文 —— 判别信息在
    /// wire 上本来就是结构化的，落库时降级成自由文本，到了 UI 又没法还原。
    #[serde(rename_all = "camelCase")]
    ResumeRejected { reason: ResumeRejectReason },

    /// 发送方的 Offer 没能送达对端（发送失败或收到非预期响应）。
    ///
    /// 两个调用点的技术细节（IO 错误、响应类型）对用户是同一件事：对方没收到你的请求。
    /// 细节进 `warn!`。
    OfferFailed,

    /// **存量数据**：本判别码引入之前写入的自由文本。
    ///
    /// 不写回填迁移 —— 失败原因是过程账本上的一句解释，重算不出来（原始错误早没了），
    /// 猜也猜不准。存量行原样展示旧串即可，新行一律是判别码；随着历史滚动它自然消失。
    /// 这与收件箱标题的处置不同（那边**回填**了），区别在于标题可以从文件列表重算，
    /// 失败原因不能。
    Legacy { message: String },
}

impl FailureCode {
    /// 编码进 `error_message` 列。
    ///
    /// `Legacy` **原样写回**而不是包成 JSON：它就是从裸串读出来的，再包一层等于
    /// 把存量格式固化进新格式里。
    pub fn to_column(&self) -> String {
        match self {
            Self::Legacy { message } => message.clone(),
            other => serde_json::to_string(other)
                // 三个变体都是纯数据、无自定义 Serialize，序列化不会失败；真失败了
                // 也不该让一次失败上报把会话卡住，降级成 Legacy 保住可展示性。
                .unwrap_or_else(|_| String::new()),
        }
    }

    /// 从 `error_message` 列解码。
    ///
    /// **解析失败不是错误，是存量数据。** 判别码引入之前所有行都是裸中文串，
    /// 它们必须继续能展示 —— 所以这里没有 `Result`，兜底永远是 `Legacy`。
    pub fn from_column(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_else(|_| Self::Legacy {
            message: raw.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_column() {
        for code in [
            FailureCode::FileFinalizeFailed {
                file_name: "季度报告.pdf".into(),
            },
            FailureCode::SessionExpired { retention_days: 7 },
        ] {
            assert_eq!(FailureCode::from_column(&code.to_column()), code);
        }
    }

    #[test]
    fn legacy_free_text_survives_both_directions() {
        // 判别码引入之前落库的行长这样。它必须原样进、原样出——包一层 JSON 就等于
        // 让存量格式在新格式里永久占一个位置。
        let raw = "文件最终化失败: a.txt (file_id=3): IO error: disk full";
        let decoded = FailureCode::from_column(raw);
        assert_eq!(
            decoded,
            FailureCode::Legacy {
                message: raw.to_string()
            }
        );
        assert_eq!(decoded.to_column(), raw);
    }

    /// 这条钉的是本判别码存在的**第二个**理由（第一个是 i18n）。
    ///
    /// 旧实现把 `file_id` 和底层错误一起拼进用户可见的串，移动端又拿整串做英文关键词
    /// 匹配，于是文件名里的 `cancel` / `network` 会决定用户看到什么原因。判别码只带
    /// 文件名一个字段，匹配这件事根本不存在了——但如果将来有人往里加 `detail: String`，
    /// 这条测试不会红。**红的是没有测试能防住的设计倒退，所以理由写在这里。**
    #[test]
    fn finalize_failure_carries_only_what_the_ui_renders() {
        let code = FailureCode::FileFinalizeFailed {
            file_name: "Q3-cancel.xlsx".into(),
        };
        let json = code.to_column();
        assert!(json.contains("Q3-cancel.xlsx"), "文件名要显示，必须在");
        assert!(!json.contains("file_id"), "file_id 只对开发者有意义");
    }
}
