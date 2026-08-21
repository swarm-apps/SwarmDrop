//! 独立文本投递 RPC 的接收端服务。

use std::future::Future;
use std::sync::{Arc, Weak};
use std::time::Duration;

use swarmdrop_net::{AcceptError, Endpoint, NodeId, PathKind, RpcService};
use uuid::Uuid;

use super::attention::TextDeliveryAttentionDeduper;
use super::pending::{PendingTextDeliveries, PendingTextDeliverySummary};
use super::{
    TEXT_DELIVERY, TEXT_DELIVERY_CONFIRMATION_TIMEOUT, TextDeliveryAttention,
    TextDeliveryAttentionKind, TextDeliveryRecord, TextDeliveryRejectReason, TextDeliveryRequest,
    TextDeliveryResponse, decode_text_body,
};
use crate::events::{TransferEvent, TransferEventSink};
use crate::host::{Notification, Notifier};
use crate::inbox::{InboxItemAddedEvent, InboxItemDetail};
use crate::manager::TransferManager;
use crate::peer::PeerDirectory;
use crate::policy::{ReceivePolicyAction, evaluate_text_receive_policy};

const MAX_PENDING_TEXT_DELIVERIES: usize = 32;

/// 文本接收服务只持有 Manager 的弱引用，避免 manager → service → manager 的 Arc 环。
#[derive(Clone)]
pub struct TextDeliveryService {
    manager: Weak<TransferManager>,
    pairing: Arc<dyn PeerDirectory>,
    endpoint: Endpoint,
    pending: Arc<PendingTextDeliveries>,
    attention: Arc<TextDeliveryAttentionDeduper>,
    notifier: Option<Arc<dyn Notifier>>,
}

impl TextDeliveryService {
    pub fn new(
        manager: Weak<TransferManager>,
        pairing: Arc<dyn PeerDirectory>,
        endpoint: Endpoint,
        notifier: Option<Arc<dyn Notifier>>,
    ) -> Self {
        Self {
            manager,
            pairing,
            endpoint,
            pending: Arc::new(PendingTextDeliveries::new(MAX_PENDING_TEXT_DELIVERIES)),
            attention: Arc::new(TextDeliveryAttentionDeduper::new()),
            notifier,
        }
    }

    pub async fn accept(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        let mut record = self.pending.begin_accept(delivery_id).await?;
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| crate::AppError::Transfer("节点已停止".into()));
        let manager = match manager {
            Ok(manager) => manager,
            Err(error) => {
                self.pending.restore_waiting(delivery_id).await;
                return Err(error);
            }
        };
        record.status = entity::TextDeliveryStatus::Delivered;
        record.updated_at = js_time_ms();
        let detail = match persist_then_publish_attention(
            manager.store.persist_incoming_text_delivery(record.clone()),
            || {
                publish_attention(
                    manager.events.as_ref(),
                    &self.attention,
                    &record,
                    TextDeliveryAttentionKind::Received,
                )
            },
        )
        .await
        {
            Ok(detail) => detail,
            Err(error) => {
                // 先落库再回复是送达语义的边界；写入失败后原 responders 与并发重发都保留。
                self.pending.restore_waiting(delivery_id).await;
                return Err(error);
            }
        };
        publish_inbox_added(manager.events.as_ref(), &detail).await;
        let response = TextDeliveryResponse::Delivered {
            inbox_item_id: detail.item.id,
        };
        for responder in self.pending.complete_accept(delivery_id).await? {
            let _ = responder.send(response.clone());
        }
        self.notify_background(&record.peer_name).await;
        Ok(())
    }

    pub async fn reject(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        self.pending.begin_reject(delivery_id).await?;
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| crate::AppError::Transfer("节点已停止".into()));
        let manager = match manager {
            Ok(manager) => manager,
            Err(error) => {
                self.pending.restore_waiting(delivery_id).await;
                return Err(error);
            }
        };
        if let Err(error) = manager
            .store
            .finalize_pending_incoming_text_delivery(
                delivery_id,
                entity::TextDeliveryStatus::Rejected,
                js_time_ms(),
            )
            .await
        {
            self.pending.restore_waiting(delivery_id).await;
            return Err(error);
        }
        for responder in self.pending.complete_reject(delivery_id).await? {
            let _ = responder.send(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::PolicyRejected,
            });
        }
        Ok(())
    }

    pub async fn pending(&self) -> crate::AppResult<Vec<PendingTextDeliverySummary>> {
        if let Some(manager) = self.manager.upgrade() {
            let now_ms = js_time_ms();
            for record in manager
                .store
                .list_pending_incoming_text_deliveries()
                .await?
            {
                if record.confirmation_window_expired(now_ms) {
                    if self.pending.expire(record.delivery_id).await
                        && let Err(error) = manager
                            .store
                            .finalize_pending_incoming_text_delivery(
                                record.delivery_id,
                                entity::TextDeliveryStatus::Expired,
                                now_ms,
                            )
                            .await
                    {
                        tracing::warn!(%error, delivery_id = %record.delivery_id, "恢复待确认文本时未能写入过期终态");
                    }
                    continue;
                }
                if let Err(reason) = self.pending.rehydrate(record).await {
                    tracing::warn!(?reason, "重建待确认文本队列时跳过无效记录");
                }
            }
        }
        Ok(self.pending.summaries().await)
    }

    fn ensure_paired_target(&self, peer: &NodeId) -> crate::AppResult<()> {
        let Some(device) = self.pairing.get_paired_device(peer) else {
            return Err(crate::AppError::Transfer("目标设备尚未配对".into()));
        };
        if device.trust_level == crate::device::DeviceTrustLevel::Blocked {
            return Err(crate::AppError::Transfer("目标设备已被阻止".into()));
        }
        Ok(())
    }

    fn via_relay(&self, peer: NodeId) -> bool {
        self.endpoint.watch_conns().with(|conns| {
            conns
                .get(&peer)
                .is_some_and(|conn| matches!(conn.path, PathKind::Relayed))
        })
    }

    async fn notify_background(&self, device_name: &str) {
        notify_background(self.notifier.as_ref(), device_name).await;
    }
}

/// 后台提醒是投递完成后的尽力而为副作用，失败只记日志，不回写投递状态。
async fn notify_background(notifier: Option<&Arc<dyn Notifier>>, device_name: &str) {
    let Some(notifier) = notifier else {
        return;
    };
    if let Err(error) = notifier
        .notify_if_unfocused(Notification::IncomingText {
            device_name: device_name.to_string(),
        })
        .await
    {
        tracing::warn!(%error, "文本后台通知发送失败，投递状态不受影响");
    }
}

impl RpcService<TextDeliveryRequest, TextDeliveryResponse> for TextDeliveryService {
    async fn handle(
        &self,
        from: NodeId,
        request: TextDeliveryRequest,
    ) -> Result<TextDeliveryResponse, AcceptError> {
        let TextDeliveryRequest::Deliver { delivery_id, body } = request;
        let Ok(body) = decode_text_body(&body) else {
            return Ok(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::InvalidPayload,
            });
        };
        let Some(manager) = self.manager.upgrade() else {
            return Ok(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::PolicyRejected,
            });
        };
        if manager.is_receiving_paused() {
            return Ok(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::ReceivingPaused,
            });
        }
        let peer = self.pairing.get_paired_device(&from);
        let decision = evaluate_text_receive_policy(
            peer.as_ref(),
            body.len() as u64,
            self.via_relay(from),
            js_time_ms(),
        );
        if decision.action == ReceivePolicyAction::Reject {
            return Ok(TextDeliveryResponse::Rejected {
                reason: if peer.is_none() {
                    TextDeliveryRejectReason::NotPaired
                } else {
                    TextDeliveryRejectReason::PolicyRejected
                },
            });
        }
        let mut record = TextDeliveryRecord {
            delivery_id,
            direction: entity::TextDeliveryDirection::Receive,
            peer_id: from.to_string(),
            peer_name: peer
                .as_ref()
                .map(|device| device.os_info.display_name())
                .unwrap_or_else(|| from.to_string()),
            body,
            status: entity::TextDeliveryStatus::WaitingConfirmation,
            failure: None,
            attempt_count: 1,
            created_at: js_time_ms(),
            updated_at: js_time_ms(),
        };
        match manager.store.get_text_delivery(delivery_id).await {
            Ok(Some(existing)) => {
                if existing.direction != entity::TextDeliveryDirection::Receive
                    || existing.peer_id != record.peer_id
                    || existing.body != record.body
                {
                    return Ok(TextDeliveryResponse::Rejected {
                        reason: TextDeliveryRejectReason::ProtocolConflict,
                    });
                }
                if existing.status == entity::TextDeliveryStatus::Delivered {
                    return match manager.store.persist_incoming_text_delivery(record).await {
                        Ok(detail) => Ok(TextDeliveryResponse::Delivered {
                            inbox_item_id: detail.item.id,
                        }),
                        Err(_) => Ok(TextDeliveryResponse::Rejected {
                            reason: TextDeliveryRejectReason::StorageUnavailable,
                        }),
                    };
                }
                if existing.status == entity::TextDeliveryStatus::Rejected {
                    return Ok(TextDeliveryResponse::Rejected {
                        reason: TextDeliveryRejectReason::PolicyRejected,
                    });
                }
                if existing.status == entity::TextDeliveryStatus::Expired {
                    return Ok(TextDeliveryResponse::Expired);
                }
            }
            Ok(None) => {}
            Err(_) => {
                return Ok(TextDeliveryResponse::Rejected {
                    reason: TextDeliveryRejectReason::StorageUnavailable,
                });
            }
        }
        if decision.action == ReceivePolicyAction::AutoAccept {
            record.status = entity::TextDeliveryStatus::Delivered;
            let attention_record = record.clone();
            return match persist_then_publish_attention(
                manager.store.persist_incoming_text_delivery(record),
                || {
                    publish_attention(
                        manager.events.as_ref(),
                        &self.attention,
                        &attention_record,
                        TextDeliveryAttentionKind::Received,
                    )
                },
            )
            .await
            {
                Ok(detail) => {
                    publish_inbox_added(manager.events.as_ref(), &detail).await;
                    self.notify_background(&attention_record.peer_name).await;
                    Ok(TextDeliveryResponse::Delivered {
                        inbox_item_id: detail.item.id,
                    })
                }
                Err(_) => Ok(TextDeliveryResponse::Rejected {
                    reason: TextDeliveryRejectReason::StorageUnavailable,
                }),
            };
        }
        let (rx, newly_queued) = match self.pending.enqueue(record.clone()).await {
            Ok(result) => result,
            Err(reason) => return Ok(TextDeliveryResponse::Rejected { reason }),
        };
        let attention_summary = newly_queued.then_some(async {
            self.pending
                .summaries()
                .await
                .into_iter()
                .find(|item| item.delivery_id == delivery_id)
        });
        let attention_summary = match attention_summary {
            Some(summary) => summary.await,
            None => None,
        };
        if persist_then_publish_attention(
            manager.store.create_pending_incoming_text_delivery(record),
            || async {
                if let Some(summary) = attention_summary.as_ref() {
                    publish_attention_summary(
                        manager.events.as_ref(),
                        &self.attention,
                        summary,
                        TextDeliveryAttentionKind::ConfirmationRequired,
                    )
                    .await;
                    self.notify_background(&summary.peer_name).await;
                }
            },
        )
        .await
        .is_err()
        {
            self.pending.discard(delivery_id).await;
            return Ok(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::StorageUnavailable,
            });
        }
        match n0_future::time::timeout(TEXT_DELIVERY_CONFIRMATION_TIMEOUT, rx).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                let _ = self.pending.expire(delivery_id).await;
                if let Err(error) = manager
                    .store
                    .finalize_pending_incoming_text_delivery(
                        delivery_id,
                        entity::TextDeliveryStatus::Expired,
                        js_time_ms(),
                    )
                    .await
                {
                    tracing::warn!(%error, %delivery_id, "文本确认超时状态未能写回账本");
                }
                Ok(TextDeliveryResponse::Expired)
            }
        }
    }
}

async fn publish_attention(
    events: &dyn TransferEventSink,
    deduper: &TextDeliveryAttentionDeduper,
    record: &TextDeliveryRecord,
    kind: TextDeliveryAttentionKind,
) {
    if !deduper.claim(record.delivery_id, kind).await {
        return;
    }
    let attention = TextDeliveryAttention {
        delivery_id: record.delivery_id,
        peer_id: record.peer_id.clone(),
        peer_name: record.peer_name.clone(),
        kind,
        created_at: record.updated_at,
    };
    if let Err(error) = events
        .emit(TransferEvent::TextDeliveryAttention { attention })
        .await
    {
        tracing::warn!(%error, "文本投递注意力事件发送失败，已持久化数据不回滚");
    }
}

async fn publish_attention_summary(
    events: &dyn TransferEventSink,
    deduper: &TextDeliveryAttentionDeduper,
    summary: &PendingTextDeliverySummary,
    kind: TextDeliveryAttentionKind,
) {
    if !deduper.claim(summary.delivery_id, kind).await {
        return;
    }
    let attention = TextDeliveryAttention {
        delivery_id: summary.delivery_id,
        peer_id: summary.peer_id.clone(),
        peer_name: summary.peer_name.clone(),
        kind,
        created_at: summary.created_at,
    };
    if let Err(error) = events
        .emit(TransferEvent::TextDeliveryAttention { attention })
        .await
    {
        tracing::warn!(%error, "文本投递注意力事件发送失败，待确认状态不回滚");
    }
}

/// 文本条目落库后通知「收件箱多了一条」。
///
/// 与 [`publish_attention`] 是两件事，不要合并：注意力信号回答的是「有人给你发了东西，
/// 要不要看一眼」（带去重，且在**待确认**阶段就会发），而这条回答的是「收件箱的内容变了」
/// ——只在条目真的落库之后发。宿主的收件箱视图订阅后者，通知栏订阅前者。
///
/// 载荷经 [`InboxItemAddedEvent::from_detail`] 投影，**不含正文**：详情里的
/// `InboxItemContent::Text { body }` 与摘要里的 `title`（正文前 160 字节）都不能进事件。
async fn publish_inbox_added(events: &dyn TransferEventSink, detail: &InboxItemDetail) {
    if let Err(error) = events
        .emit(TransferEvent::InboxItemAdded {
            event: InboxItemAddedEvent::from_detail(detail),
        })
        .await
    {
        tracing::warn!(%error, "收件箱新增事件发送失败，已持久化数据不回滚");
    }
}

/// 注意力只是已持久化账本的即时提示，不能抢在账本之前成为用户可见事实。
async fn persist_then_publish_attention<T, Persist, Publish, PublishFuture>(
    persistence: Persist,
    publish: Publish,
) -> crate::AppResult<T>
where
    Persist: Future<Output = crate::AppResult<T>>,
    Publish: FnOnce() -> PublishFuture,
    PublishFuture: Future<Output = ()>,
{
    let value = persistence.await?;
    publish().await;
    Ok(value)
}

fn js_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl TransferManager {
    /// 创建发送账本后才发起网络调用；调用返回值代表接收端已持久化，而非仅已发包。
    pub async fn send_text_delivery(
        self: &Arc<Self>,
        peer_id: String,
        peer_name: String,
        body: String,
    ) -> crate::AppResult<TextDeliveryRecord> {
        let record = TextDeliveryRecord::new_outgoing(
            Uuid::new_v4(),
            peer_id,
            peer_name,
            body,
            js_time_ms(),
        )?;
        self.store
            .create_outgoing_text_delivery(record.clone())
            .await?;
        self.deliver_text_record(record).await
    }

    pub async fn retry_text_delivery(
        self: &Arc<Self>,
        delivery_id: Uuid,
    ) -> crate::AppResult<TextDeliveryRecord> {
        let mut record = self
            .store
            .get_text_delivery(delivery_id)
            .await?
            .ok_or_else(|| crate::AppError::Transfer("文本投递记录不存在".into()))?;
        if !record.is_retryable() {
            return Err(crate::AppError::Transfer("该文本投递当前不可重试".into()));
        }
        record.status = entity::TextDeliveryStatus::Sending;
        record.failure = None;
        record.attempt_count += 1;
        record.updated_at = js_time_ms();
        self.store
            .update_outgoing_text_delivery(
                record.delivery_id,
                record.status.clone(),
                None,
                Some(record.attempt_count),
                record.updated_at,
            )
            .await?;
        self.deliver_text_record(record).await
    }

    async fn deliver_text_record(
        self: &Arc<Self>,
        record: TextDeliveryRecord,
    ) -> crate::AppResult<TextDeliveryRecord> {
        let peer = record
            .peer_id
            .parse::<NodeId>()
            .map_err(|_| crate::AppError::Transfer("目标设备标识无效".into()))?;
        self.text_delivery_service()?.ensure_paired_target(&peer)?;
        self.store
            .update_outgoing_text_delivery(
                record.delivery_id,
                entity::TextDeliveryStatus::WaitingConfirmation,
                None,
                Some(record.attempt_count),
                js_time_ms(),
            )
            .await?;
        let outcome = TEXT_DELIVERY
            .call_with(
                &self.endpoint,
                peer,
                &TextDeliveryRequest::Deliver {
                    delivery_id: record.delivery_id,
                    body: record.body.as_bytes().to_vec(),
                },
                swarmdrop_net::CallOptions {
                    timeout: Duration::from_secs(305),
                },
            )
            .await;
        let (status, failure) = match outcome {
            Ok(TextDeliveryResponse::Delivered { .. }) => {
                (entity::TextDeliveryStatus::Delivered, None)
            }
            Ok(TextDeliveryResponse::Rejected { reason }) => match reason {
                TextDeliveryRejectReason::ReceivingPaused | TextDeliveryRejectReason::QueueFull => {
                    (
                        entity::TextDeliveryStatus::Retryable,
                        Some(entity::TextDeliveryFailure::PeerUnavailable),
                    )
                }
                TextDeliveryRejectReason::StorageUnavailable => (
                    entity::TextDeliveryStatus::Retryable,
                    Some(entity::TextDeliveryFailure::StorageFailed),
                ),
                _ => (
                    entity::TextDeliveryStatus::Rejected,
                    Some(entity::TextDeliveryFailure::Rejected),
                ),
            },
            Ok(TextDeliveryResponse::Expired) => (
                entity::TextDeliveryStatus::Expired,
                Some(entity::TextDeliveryFailure::Expired),
            ),
            Err(_) => (
                entity::TextDeliveryStatus::Retryable,
                Some(entity::TextDeliveryFailure::TimedOut),
            ),
        };
        let updated_at = js_time_ms();
        self.store
            .update_outgoing_text_delivery(record.delivery_id, status, failure, None, updated_at)
            .await?;
        self.store
            .get_text_delivery(record.delivery_id)
            .await?
            .ok_or_else(|| crate::AppError::Transfer("文本投递记录在更新后丢失".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use entity::{TextDeliveryDirection, TextDeliveryStatus};
    use uuid::Uuid;

    use super::super::attention::TextDeliveryAttentionDeduper;
    use super::{
        PendingTextDeliveries, TextDeliveryAttentionKind, TextDeliveryRecord,
        TextDeliveryRejectReason, TextDeliveryResponse, persist_then_publish_attention,
        publish_attention,
    };
    use crate::events::{TransferEvent, TransferEventSink};
    use crate::host::{Notification, Notifier};

    #[derive(Default)]
    struct RecordingEvents {
        events: Mutex<Vec<TransferEvent>>,
        fail: bool,
    }

    #[async_trait]
    impl TransferEventSink for RecordingEvents {
        async fn emit(&self, event: TransferEvent) -> crate::AppResult<()> {
            if self.fail {
                return Err(crate::AppError::Transfer("模拟事件通道失败".into()));
            }
            self.events.lock().expect("锁定测试事件").push(event);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailingNotifier {
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl Notifier for FailingNotifier {
        async fn notify(&self, _notification: Notification) -> crate::AppResult<()> {
            *self.calls.lock().expect("记录通知调用") += 1;
            Err(crate::AppError::Transfer("模拟通知失败".into()))
        }
    }

    fn incoming(delivery_id: Uuid, peer_id: &str, body: &str) -> TextDeliveryRecord {
        TextDeliveryRecord {
            delivery_id,
            direction: TextDeliveryDirection::Receive,
            peer_id: peer_id.into(),
            peer_name: "Alice".into(),
            body: body.into(),
            status: TextDeliveryStatus::Delivered,
            failure: None,
            attempt_count: 1,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn pending_queue_is_bounded_and_binds_id_to_peer_and_body() {
        let queue = PendingTextDeliveries::new(1);
        let delivery_id = Uuid::new_v4();
        let (_first, _) = queue
            .enqueue(incoming(delivery_id, "peer-a", "first"))
            .await
            .expect("首次请求应进入队列");

        assert_eq!(
            queue
                .enqueue(incoming(delivery_id, "peer-b", "first"))
                .await
                .expect_err("同一标识不同来源必须拒绝"),
            TextDeliveryRejectReason::ProtocolConflict,
        );
        assert_eq!(
            queue
                .enqueue(incoming(Uuid::new_v4(), "peer-a", "second"))
                .await
                .expect_err("上限必须在单一临界区内生效"),
            TextDeliveryRejectReason::QueueFull,
        );
    }

    #[tokio::test]
    async fn pending_queue_keeps_arrival_order_instead_of_sorting_delivery_ids() {
        let queue = PendingTextDeliveries::new(2);
        let first_id = Uuid::from_u128(u128::MAX);
        let second_id = Uuid::from_u128(1);
        let (_first, _) = queue
            .enqueue(incoming(first_id, "peer-a", "first"))
            .await
            .expect("首个请求");
        let (_second, _) = queue
            .enqueue(incoming(second_id, "peer-a", "second"))
            .await
            .expect("第二个请求");

        let ids: Vec<_> = queue
            .summaries()
            .await
            .into_iter()
            .map(|summary| summary.delivery_id)
            .collect();
        assert_eq!(ids, vec![first_id, second_id], "展示顺序必须遵循到达顺序");
    }

    #[tokio::test]
    async fn reconciliation_rehydrates_once_without_creating_phantom_rpc_responders() {
        let queue = PendingTextDeliveries::new(1);
        let delivery_id = Uuid::new_v4();
        let record = incoming(delivery_id, "peer-a", "recovered");

        assert!(queue.rehydrate(record.clone()).await.expect("首次恢复"));
        assert!(
            !queue.rehydrate(record).await.expect("重复恢复"),
            "协调读取不得重复排队"
        );
        queue.begin_accept(delivery_id).await.expect("开始确认");
        assert!(
            queue
                .complete_accept(delivery_id)
                .await
                .expect("完成确认")
                .is_empty(),
            "恢复条目没有等待中的 RPC，不得凭空生成 responder"
        );
    }

    #[tokio::test]
    async fn accept_and_reject_compete_for_one_terminal_result() {
        let queue = PendingTextDeliveries::new(1);
        let delivery_id = Uuid::new_v4();
        let (_receiver, _) = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("首个请求");

        let (accept, reject) = tokio::join!(
            queue.begin_accept(delivery_id),
            queue.begin_reject(delivery_id),
        );
        assert!(
            accept.is_ok() ^ reject.is_ok(),
            "确认与拒绝必须只有一个操作取得终局所有权"
        );
    }

    #[tokio::test]
    async fn attention_event_identifies_delivery_without_exposing_its_body() {
        let events = RecordingEvents::default();
        let deduper = TextDeliveryAttentionDeduper::new();
        let record = incoming(Uuid::new_v4(), "peer-a", "敏感正文");

        publish_attention(
            &events,
            &deduper,
            &record,
            TextDeliveryAttentionKind::Received,
        )
        .await;
        publish_attention(
            &events,
            &deduper,
            &record,
            TextDeliveryAttentionKind::Received,
        )
        .await;

        let emitted = events.events.lock().expect("读取测试事件");
        let [TransferEvent::TextDeliveryAttention { attention }] = emitted.as_slice() else {
            panic!("必须发出一个文本注意力事件");
        };
        assert_eq!(attention.delivery_id, record.delivery_id);
        assert_eq!(attention.kind, TextDeliveryAttentionKind::Received);
        assert!(
            serde_json::to_value(attention)
                .expect("序列化注意力事件")
                .get("body")
                .is_none(),
            "注意力事件不得包含文本正文"
        );
    }

    #[tokio::test]
    async fn failed_attention_event_does_not_mutate_the_persisted_record_input() {
        let events = RecordingEvents {
            fail: true,
            ..Default::default()
        };
        let deduper = TextDeliveryAttentionDeduper::new();
        let record = incoming(Uuid::new_v4(), "peer-a", "正文");

        publish_attention(
            &events,
            &deduper,
            &record,
            TextDeliveryAttentionKind::Received,
        )
        .await;

        assert_eq!(record.status, TextDeliveryStatus::Delivered);
        assert_eq!(record.body, "正文");
    }

    #[tokio::test]
    async fn failed_background_notification_does_not_mutate_delivered_record() {
        let notifier = Arc::new(FailingNotifier::default());
        let notifier_port: Arc<dyn Notifier> = notifier.clone();
        let record = incoming(Uuid::new_v4(), "peer-a", "正文");
        let status_before_notification = record.status.clone();

        super::notify_background(Some(&notifier_port), &record.peer_name).await;

        assert_eq!(record.status, status_before_notification);
        assert_eq!(record.body, "正文");
        assert_eq!(
            *notifier.calls.lock().expect("读取通知调用"),
            1,
            "通知失败只能被记录一次，不能触发投递重试或状态回滚"
        );
    }

    #[tokio::test]
    async fn persistence_completes_before_attention_is_published() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let persist_order = Arc::clone(&order);
        let publish_order = Arc::clone(&order);

        persist_then_publish_attention(
            async move {
                persist_order
                    .lock()
                    .expect("记录持久化顺序")
                    .push("persist");
                Ok::<_, crate::AppError>(())
            },
            move || async move {
                publish_order.lock().expect("记录事件顺序").push("publish");
            },
        )
        .await
        .expect("持久化成功后应发出注意力");

        assert_eq!(
            *order.lock().expect("读取执行顺序"),
            ["persist", "publish"],
            "若将事件提到持久化之前，这个测试必须失败"
        );
    }

    #[tokio::test]
    async fn failed_persistence_never_publishes_attention() {
        let published = Arc::new(Mutex::new(false));
        let published_from_callback = Arc::clone(&published);

        let result = persist_then_publish_attention(
            async { Err::<(), _>(crate::AppError::Transfer("模拟存储失败".into())) },
            move || async move {
                *published_from_callback.lock().expect("记录事件是否发送") = true;
            },
        )
        .await;

        assert!(result.is_err());
        assert!(
            !*published.lock().expect("读取事件是否发送"),
            "持久化失败时不得产生指向不存在账本记录的注意力事件"
        );
    }

    #[tokio::test]
    async fn duplicate_during_confirmation_survives_storage_failure_and_gets_one_result() {
        let queue = PendingTextDeliveries::new(2);
        let delivery_id = Uuid::new_v4();
        let (first, _) = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("首次请求");
        let record = queue.begin_accept(delivery_id).await.expect("开始确认");
        assert_eq!(record.body, "body");

        // 模拟落库期间的回执丢失重发：它必须合并而不是生成第二个待确认条目。
        let (retry, _) = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("同内容重试合并");
        queue.restore_waiting(delivery_id).await;
        assert_eq!(queue.summaries().await.len(), 1, "落库失败后仍可再次确认");

        queue.begin_accept(delivery_id).await.expect("再次确认");
        let responders = queue.complete_accept(delivery_id).await.expect("完成确认");
        let response = TextDeliveryResponse::Delivered {
            inbox_item_id: Uuid::new_v4(),
        };
        for responder in responders {
            let _ = responder.send(response.clone());
        }
        assert_eq!(first.await.expect("首个回执"), response);
        assert_eq!(retry.await.expect("重试回执"), response);
    }

    #[tokio::test]
    async fn expiry_cannot_remove_a_request_after_confirmation_started() {
        let queue = PendingTextDeliveries::new(1);
        let delivery_id = Uuid::new_v4();
        let (_receiver, _) = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("首次请求");
        queue.begin_accept(delivery_id).await.expect("开始确认");
        assert!(
            !queue.expire(delivery_id).await,
            "确认已开始时，过期协调不得夺取终态写入权"
        );

        assert_eq!(
            queue
                .complete_accept(delivery_id)
                .await
                .expect("确认中的条目不能被 TTL 任务删除")
                .len(),
            1,
        );
    }
}
