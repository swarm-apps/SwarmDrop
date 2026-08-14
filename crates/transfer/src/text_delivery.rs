//! 纯文本投递的领域模型与状态规则。
//!
//! 文本投递不复用文件会话：它没有路径、分块、恢复数据面或后台续传。这里保留的只有
//! 可重试的本地事实、64 KiB 输入边界及状态转换，网络和持久化分别走协议与端口层。

use entity::{TextDeliveryDirection, TextDeliveryFailure, TextDeliveryStatus};
use uuid::Uuid;

use crate::{AppError, AppResult};

/// UTF-8 正文的最大字节数。
pub const MAX_TEXT_DELIVERY_BYTES: usize = 64 * 1024;

/// 文本列表摘要的最大 UTF-8 字节数。
pub const TEXT_DELIVERY_PREVIEW_BYTES: usize = 160;

/// 一条收发两侧共用的文本投递账本记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TextDeliveryRecord {
    pub delivery_id: Uuid,
    pub direction: TextDeliveryDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub body: String,
    pub status: TextDeliveryStatus,
    pub failure: Option<TextDeliveryFailure>,
    pub attempt_count: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TextDeliveryRecord {
    /// 新建一条尚未发出网络请求的发送记录。
    pub fn new_outgoing(
        delivery_id: Uuid,
        peer_id: String,
        peer_name: String,
        body: String,
        now_ms: i64,
    ) -> AppResult<Self> {
        validate_text_body(&body)?;
        Ok(Self {
            delivery_id,
            direction: TextDeliveryDirection::Send,
            peer_id,
            peer_name,
            body,
            status: TextDeliveryStatus::Sending,
            failure: None,
            attempt_count: 1,
            created_at: now_ms,
            updated_at: now_ms,
        })
    }

    /// 返回不会截断 UTF-8 代码点的受限预览。
    pub fn preview(&self) -> String {
        text_preview(&self.body)
    }

    /// 只有未获得确定送达结果的发送记录可用同一幂等键重试。
    pub fn is_retryable(&self) -> bool {
        self.direction == TextDeliveryDirection::Send
            && matches!(
                self.status,
                TextDeliveryStatus::Retryable
                    | TextDeliveryStatus::Expired
                    | TextDeliveryStatus::WaitingConfirmation
            )
    }

    /// 进程重启不能重发敏感正文，只把在途状态恢复为用户可见的显式重试。
    pub fn recover_after_restart(&mut self, now_ms: i64) -> bool {
        if self.direction != TextDeliveryDirection::Send
            || !matches!(
                self.status,
                TextDeliveryStatus::Sending | TextDeliveryStatus::WaitingConfirmation
            )
        {
            return false;
        }
        self.status = TextDeliveryStatus::Retryable;
        self.failure = Some(TextDeliveryFailure::TimedOut);
        self.updated_at = now_ms;
        true
    }
}

/// 校验本地输入或 wire 解码得到的 UTF-8 正文。
pub fn validate_text_body(body: &str) -> AppResult<()> {
    if body.is_empty() {
        return Err(AppError::Transfer("文本内容不能为空".into()));
    }
    if body.len() > MAX_TEXT_DELIVERY_BYTES {
        return Err(AppError::Transfer(format!(
            "文本内容不能超过 {MAX_TEXT_DELIVERY_BYTES} 字节"
        )));
    }
    Ok(())
}

/// 从 wire 字节解码并校验正文，明确拒绝非 UTF-8 而不是有损替换。
pub fn decode_text_body(bytes: &[u8]) -> AppResult<String> {
    let body = std::str::from_utf8(bytes)
        .map_err(|_| AppError::Transfer("文本内容不是合法 UTF-8".into()))?;
    validate_text_body(body)?;
    Ok(body.to_owned())
}

/// 将文本裁剪到固定字节预算，且永远保留合法 UTF-8。
pub fn text_preview(body: &str) -> String {
    let mut end = body.len().min(TEXT_DELIVERY_PREVIEW_BYTES);
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &body[..end];
    if end == body.len() {
        prefix.to_owned()
    } else {
        format!("{prefix}…")
    }
}

#[cfg(test)]
mod tests {
    use entity::{TextDeliveryDirection, TextDeliveryFailure, TextDeliveryStatus};
    use uuid::Uuid;

    use super::{
        MAX_TEXT_DELIVERY_BYTES, TextDeliveryRecord, decode_text_body, text_preview,
        validate_text_body,
    };

    #[test]
    fn validates_empty_and_64_kib_boundaries() {
        assert!(validate_text_body("").is_err());
        assert!(validate_text_body(&"a".repeat(MAX_TEXT_DELIVERY_BYTES)).is_ok());
        assert!(validate_text_body(&"a".repeat(MAX_TEXT_DELIVERY_BYTES + 1)).is_err());
    }

    #[test]
    fn rejects_invalid_utf8_from_wire() {
        assert!(decode_text_body(&[0xff]).is_err());
    }

    #[test]
    fn preview_never_splits_a_utf8_codepoint() {
        let text = "界".repeat(100);
        let preview = text_preview(&text);
        assert!(preview.is_char_boundary(preview.len()));
        assert_eq!(preview, format!("{}…", "界".repeat(53)));

        // 预览预算恰好用尽时仍是完整内容；`>=` 会错误追加省略号。
        assert_eq!(text_preview(&"a".repeat(160)), "a".repeat(160));
    }

    #[test]
    fn restart_only_makes_inflight_outbox_records_retryable() {
        let mut record = TextDeliveryRecord::new_outgoing(
            Uuid::new_v4(),
            "peer".into(),
            "设备".into(),
            "正文".into(),
            1,
        )
        .expect("valid record");

        assert!(record.recover_after_restart(2));
        assert_eq!(record.status, TextDeliveryStatus::Retryable);
        assert_eq!(record.failure, Some(TextDeliveryFailure::TimedOut));
        assert!(record.is_retryable());

        record.direction = TextDeliveryDirection::Receive;
        record.status = TextDeliveryStatus::Sending;
        assert!(
            !record.recover_after_restart(3),
            "接收方向绝不能被启动恢复误标为可重试"
        );

        record.direction = TextDeliveryDirection::Send;
        record.status = TextDeliveryStatus::Delivered;
        assert!(
            !record.recover_after_restart(3),
            "终态记录绝不能被启动恢复改变"
        );
    }

    #[test]
    fn outgoing_record_and_retryability_keep_directional_invariants() {
        let mut record = TextDeliveryRecord::new_outgoing(
            Uuid::new_v4(),
            "peer".into(),
            "设备".into(),
            "正文".into(),
            7,
        )
        .expect("valid record");
        assert_eq!(record.direction, TextDeliveryDirection::Send);
        assert_eq!(record.status, TextDeliveryStatus::Sending);
        assert_eq!(record.attempt_count, 1);
        assert!(!record.is_retryable(), "发送中不能被用户重试");

        record.status = TextDeliveryStatus::Delivered;
        assert!(!record.is_retryable(), "已送达不能被用户重试");
        assert_eq!(record.preview(), "正文");

        record.body = "x".repeat(200);
        assert_eq!(record.preview(), format!("{}…", "x".repeat(160)));
    }
}
