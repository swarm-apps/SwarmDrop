//! 文本投递到达后的宿主注意力事件。

use std::collections::HashSet;

use tokio::sync::Mutex;
use uuid::Uuid;

/// 用户需要处理的文本投递状态。事件不含正文，避免系统通知或日志泄露敏感内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TextDeliveryAttentionKind {
    ConfirmationRequired,
    Received,
}

/// 已持久化或已进入待确认队列的文本投递注意力。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TextDeliveryAttention {
    pub delivery_id: Uuid,
    pub peer_id: String,
    pub peer_name: String,
    pub kind: TextDeliveryAttentionKind,
    pub created_at: i64,
}

/// 进程存活期间只允许同一投递状态发出一次即时提示；宿主重新挂载仍可通过持久化读取协调。
pub(super) struct TextDeliveryAttentionDeduper {
    emitted: Mutex<HashSet<(Uuid, TextDeliveryAttentionKind)>>,
}

impl TextDeliveryAttentionDeduper {
    pub(super) fn new() -> Self {
        Self {
            emitted: Mutex::new(HashSet::new()),
        }
    }

    pub(super) async fn claim(&self, delivery_id: Uuid, kind: TextDeliveryAttentionKind) -> bool {
        self.emitted.lock().await.insert((delivery_id, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::{TextDeliveryAttentionDeduper, TextDeliveryAttentionKind};
    use uuid::Uuid;

    #[tokio::test]
    async fn only_first_delivery_of_the_same_attention_kind_claims_the_signal() {
        let deduper = TextDeliveryAttentionDeduper::new();
        let delivery_id = Uuid::new_v4();

        assert!(
            deduper
                .claim(delivery_id, TextDeliveryAttentionKind::ConfirmationRequired)
                .await
        );
        assert!(
            !deduper
                .claim(delivery_id, TextDeliveryAttentionKind::ConfirmationRequired)
                .await,
            "重复 RPC 不得生成第二次即时注意力"
        );
        assert!(
            deduper
                .claim(delivery_id, TextDeliveryAttentionKind::Received)
                .await,
            "不同注意力状态仍必须可以发出"
        );
    }
}
