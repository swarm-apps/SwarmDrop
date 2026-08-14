//! 需要用户确认的文本投递并发状态机。

use std::collections::{BTreeMap, VecDeque};

use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

use super::{TextDeliveryRecord, TextDeliveryRejectReason, TextDeliveryResponse};

struct PendingTextDelivery {
    record: TextDeliveryRecord,
    responders: Vec<oneshot::Sender<TextDeliveryResponse>>,
}

/// 确认写入期间仍保留条目，使相同投递的重发可合并到同一组回执，而不会与失败恢复竞争。
enum PendingTextDeliveryState {
    Waiting(PendingTextDelivery),
    Accepting(PendingTextDelivery),
}

/// 待确认文本的单一并发边界。
///
/// 使用一个短临界区而非 `DashMap::len() + entry()` 的组合：后者在并发入站时无法原子地
/// 维护上限，也会在落库失败恢复时覆盖随后抵达的同 ID 重发请求。
pub(super) struct PendingTextDeliveries {
    state: Mutex<PendingTextDeliveryQueue>,
    max_entries: usize,
}

struct PendingTextDeliveryQueue {
    entries: BTreeMap<Uuid, PendingTextDeliveryState>,
    order: VecDeque<Uuid>,
}

impl PendingTextDeliveryQueue {
    fn remove(&mut self, delivery_id: Uuid) -> Option<PendingTextDeliveryState> {
        let entry = self.entries.remove(&delivery_id)?;
        if let Some(index) = self.order.iter().position(|id| *id == delivery_id) {
            self.order.remove(index);
        }
        Some(entry)
    }
}

impl PendingTextDeliveries {
    pub(super) fn new(max_entries: usize) -> Self {
        Self {
            state: Mutex::new(PendingTextDeliveryQueue {
                entries: BTreeMap::new(),
                order: VecDeque::new(),
            }),
            max_entries,
        }
    }

    pub(super) async fn enqueue(
        &self,
        record: TextDeliveryRecord,
    ) -> Result<(oneshot::Receiver<TextDeliveryResponse>, bool), TextDeliveryRejectReason> {
        let (tx, rx) = oneshot::channel();
        let mut state = self.state.lock().await;
        match state.entries.get_mut(&record.delivery_id) {
            Some(entry) => {
                let pending = match entry {
                    PendingTextDeliveryState::Waiting(pending)
                    | PendingTextDeliveryState::Accepting(pending) => pending,
                };
                if pending.record.peer_id != record.peer_id || pending.record.body != record.body {
                    return Err(TextDeliveryRejectReason::ProtocolConflict);
                }
                pending.responders.push(tx);
                return Ok((rx, false));
            }
            None => {
                if state.entries.len() >= self.max_entries {
                    return Err(TextDeliveryRejectReason::QueueFull);
                }
                let delivery_id = record.delivery_id;
                state.entries.insert(
                    delivery_id,
                    PendingTextDeliveryState::Waiting(PendingTextDelivery {
                        record,
                        responders: vec![tx],
                    }),
                );
                state.order.push_back(delivery_id);
            }
        }
        Ok((rx, true))
    }

    /// 从持久化账本重建队列时不附加 RPC 回执；否则每次 UI 协调读取都会累积一个永远无人接收的 sender。
    pub(super) async fn rehydrate(
        &self,
        record: TextDeliveryRecord,
    ) -> Result<bool, TextDeliveryRejectReason> {
        let mut state = self.state.lock().await;
        if let Some(entry) = state.entries.get(&record.delivery_id) {
            let pending = match entry {
                PendingTextDeliveryState::Waiting(pending)
                | PendingTextDeliveryState::Accepting(pending) => pending,
            };
            if pending.record.peer_id != record.peer_id || pending.record.body != record.body {
                return Err(TextDeliveryRejectReason::ProtocolConflict);
            }
            return Ok(false);
        }
        if state.entries.len() >= self.max_entries {
            return Err(TextDeliveryRejectReason::QueueFull);
        }
        let delivery_id = record.delivery_id;
        state.entries.insert(
            delivery_id,
            PendingTextDeliveryState::Waiting(PendingTextDelivery {
                record,
                responders: Vec::new(),
            }),
        );
        state.order.push_back(delivery_id);
        Ok(true)
    }

    pub(super) async fn begin_accept(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<TextDeliveryRecord> {
        let mut state = self.state.lock().await;
        let Some(entry) = state.entries.remove(&delivery_id) else {
            return Err(crate::AppError::Transfer(
                "待确认文本投递不存在或已过期".into(),
            ));
        };
        let pending = match entry {
            PendingTextDeliveryState::Waiting(pending) => pending,
            PendingTextDeliveryState::Accepting(pending) => {
                state
                    .entries
                    .insert(delivery_id, PendingTextDeliveryState::Accepting(pending));
                return Err(crate::AppError::Transfer("文本投递正在确认中".into()));
            }
        };
        let record = pending.record.clone();
        state
            .entries
            .insert(delivery_id, PendingTextDeliveryState::Accepting(pending));
        Ok(record)
    }

    pub(super) async fn begin_reject(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<TextDeliveryRecord> {
        self.begin_accept(delivery_id).await
    }

    pub(super) async fn restore_waiting(&self, delivery_id: Uuid) {
        let mut state = self.state.lock().await;
        let Some(entry) = state.entries.remove(&delivery_id) else {
            return;
        };
        let pending = match entry {
            PendingTextDeliveryState::Waiting(pending)
            | PendingTextDeliveryState::Accepting(pending) => pending,
        };
        state
            .entries
            .insert(delivery_id, PendingTextDeliveryState::Waiting(pending));
    }

    pub(super) async fn complete_accept(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<Vec<oneshot::Sender<TextDeliveryResponse>>> {
        let mut state = self.state.lock().await;
        let Some(entry) = state.remove(delivery_id) else {
            return Err(crate::AppError::Transfer(
                "待确认文本投递不存在或已过期".into(),
            ));
        };
        match entry {
            PendingTextDeliveryState::Accepting(pending) => Ok(pending.responders),
            PendingTextDeliveryState::Waiting(pending) => {
                state
                    .entries
                    .insert(delivery_id, PendingTextDeliveryState::Waiting(pending));
                state.order.push_front(delivery_id);
                Err(crate::AppError::Transfer("文本投递尚未开始确认".into()))
            }
        }
    }

    pub(super) async fn complete_reject(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<Vec<oneshot::Sender<TextDeliveryResponse>>> {
        self.complete_accept(delivery_id).await
    }

    pub(super) async fn summaries(&self) -> Vec<PendingTextDeliverySummary> {
        let state = self.state.lock().await;
        state
            .order
            .iter()
            .filter_map(|delivery_id| {
                state
                    .entries
                    .get(delivery_id)
                    .map(|entry| (delivery_id, entry))
            })
            .map(|(delivery_id, entry)| {
                let pending = match entry {
                    PendingTextDeliveryState::Waiting(pending)
                    | PendingTextDeliveryState::Accepting(pending) => pending,
                };
                PendingTextDeliverySummary {
                    delivery_id: *delivery_id,
                    peer_id: pending.record.peer_id.clone(),
                    peer_name: pending.record.peer_name.clone(),
                    body: pending.record.body.clone(),
                    created_at: pending.record.created_at,
                }
            })
            .collect()
    }

    /// 只回收尚未开始持久化的请求；确认动作一旦开始就不能被超时任务从脚下删掉。
    /// 返回 `false` 表示确认已开始，调用方必须让确认流程取得终态写入权。
    pub(super) async fn expire(&self, delivery_id: Uuid) -> bool {
        let mut state = self.state.lock().await;
        match state.entries.get(&delivery_id) {
            Some(PendingTextDeliveryState::Waiting(_)) => {
                state.remove(delivery_id);
                true
            }
            Some(PendingTextDeliveryState::Accepting(_)) => false,
            None => true,
        }
    }

    /// 仅在持久化尚未成功时撤回排队项；不会向对端伪造送达结果。
    pub(super) async fn discard(&self, delivery_id: Uuid) {
        let mut state = self.state.lock().await;
        if matches!(
            state.entries.get(&delivery_id),
            Some(PendingTextDeliveryState::Waiting(_))
        ) {
            state.remove(delivery_id);
        }
    }
}

/// 等待用户确认的文本投递快照。正文在本地内存中仅保留到确认窗口结束，绝不预先写入剪贴板。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PendingTextDeliverySummary {
    pub delivery_id: Uuid,
    pub peer_id: String,
    pub peer_name: String,
    pub body: String,
    pub created_at: i64,
}
