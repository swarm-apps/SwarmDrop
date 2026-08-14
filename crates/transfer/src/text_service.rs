//! 独立文本投递 RPC 的接收端服务。

use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use swarmdrop_net::{AcceptError, Endpoint, NodeId, PathKind, RpcService};
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

use crate::manager::TransferManager;
use crate::peer::PeerDirectory;
use crate::policy::{ReceivePolicyAction, evaluate_text_receive_policy};
use crate::protocol::{TextDeliveryRejectReason, TextDeliveryRequest, TextDeliveryResponse};
use crate::text_delivery::{TextDeliveryRecord, decode_text_body};

const MAX_PENDING_TEXT_DELIVERIES: usize = 32;

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
struct PendingTextDeliveries {
    entries: Mutex<BTreeMap<Uuid, PendingTextDeliveryState>>,
    max_entries: usize,
}

impl PendingTextDeliveries {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            max_entries,
        }
    }

    async fn enqueue(
        &self,
        record: TextDeliveryRecord,
    ) -> Result<oneshot::Receiver<TextDeliveryResponse>, TextDeliveryRejectReason> {
        let (tx, rx) = oneshot::channel();
        let mut entries = self.entries.lock().await;
        match entries.get_mut(&record.delivery_id) {
            Some(entry) => {
                let pending = match entry {
                    PendingTextDeliveryState::Waiting(pending)
                    | PendingTextDeliveryState::Accepting(pending) => pending,
                };
                if pending.record.peer_id != record.peer_id || pending.record.body != record.body {
                    return Err(TextDeliveryRejectReason::ProtocolConflict);
                }
                pending.responders.push(tx);
            }
            None => {
                if entries.len() >= self.max_entries {
                    return Err(TextDeliveryRejectReason::QueueFull);
                }
                entries.insert(
                    record.delivery_id,
                    PendingTextDeliveryState::Waiting(PendingTextDelivery {
                        record,
                        responders: vec![tx],
                    }),
                );
            }
        }
        Ok(rx)
    }

    async fn begin_accept(&self, delivery_id: Uuid) -> crate::AppResult<TextDeliveryRecord> {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.remove(&delivery_id) else {
            return Err(crate::AppError::Transfer(
                "待确认文本投递不存在或已过期".into(),
            ));
        };
        let pending = match entry {
            PendingTextDeliveryState::Waiting(pending) => pending,
            PendingTextDeliveryState::Accepting(pending) => {
                entries.insert(delivery_id, PendingTextDeliveryState::Accepting(pending));
                return Err(crate::AppError::Transfer("文本投递正在确认中".into()));
            }
        };
        let record = pending.record.clone();
        entries.insert(delivery_id, PendingTextDeliveryState::Accepting(pending));
        Ok(record)
    }

    async fn restore_waiting(&self, delivery_id: Uuid) {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.remove(&delivery_id) else {
            return;
        };
        let pending = match entry {
            PendingTextDeliveryState::Waiting(pending)
            | PendingTextDeliveryState::Accepting(pending) => pending,
        };
        entries.insert(delivery_id, PendingTextDeliveryState::Waiting(pending));
    }

    async fn complete_accept(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<Vec<oneshot::Sender<TextDeliveryResponse>>> {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.remove(&delivery_id) else {
            return Err(crate::AppError::Transfer(
                "待确认文本投递不存在或已过期".into(),
            ));
        };
        match entry {
            PendingTextDeliveryState::Accepting(pending) => Ok(pending.responders),
            PendingTextDeliveryState::Waiting(pending) => {
                entries.insert(delivery_id, PendingTextDeliveryState::Waiting(pending));
                Err(crate::AppError::Transfer("文本投递尚未开始确认".into()))
            }
        }
    }

    async fn reject(
        &self,
        delivery_id: Uuid,
    ) -> crate::AppResult<Vec<oneshot::Sender<TextDeliveryResponse>>> {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.remove(&delivery_id) else {
            return Err(crate::AppError::Transfer(
                "待确认文本投递不存在或已过期".into(),
            ));
        };
        match entry {
            PendingTextDeliveryState::Waiting(pending) => Ok(pending.responders),
            PendingTextDeliveryState::Accepting(pending) => {
                entries.insert(delivery_id, PendingTextDeliveryState::Accepting(pending));
                Err(crate::AppError::Transfer("文本投递正在确认中".into()))
            }
        }
    }

    async fn summaries(&self) -> Vec<PendingTextDeliverySummary> {
        self.entries
            .lock()
            .await
            .iter()
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
    async fn expire(&self, delivery_id: Uuid) {
        let mut entries = self.entries.lock().await;
        if matches!(
            entries.get(&delivery_id),
            Some(PendingTextDeliveryState::Waiting(_))
        ) {
            entries.remove(&delivery_id);
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

/// 文本接收服务只持有 Manager 的弱引用，避免 manager → service → manager 的 Arc 环。
#[derive(Clone)]
pub struct TextDeliveryService {
    manager: Weak<TransferManager>,
    pairing: Arc<dyn PeerDirectory>,
    endpoint: Endpoint,
    pending: Arc<PendingTextDeliveries>,
}

impl TextDeliveryService {
    pub fn new(
        manager: Weak<TransferManager>,
        pairing: Arc<dyn PeerDirectory>,
        endpoint: Endpoint,
    ) -> Self {
        Self {
            manager,
            pairing,
            endpoint,
            pending: Arc::new(PendingTextDeliveries::new(MAX_PENDING_TEXT_DELIVERIES)),
        }
    }

    pub async fn accept(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        let record = self.pending.begin_accept(delivery_id).await?;
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
        let detail = match manager.store.persist_incoming_text_delivery(record).await {
            Ok(detail) => detail,
            Err(error) => {
                // 先落库再回复是送达语义的边界；写入失败后原 responders 与并发重发都保留。
                self.pending.restore_waiting(delivery_id).await;
                return Err(error);
            }
        };
        let response = TextDeliveryResponse::Delivered {
            inbox_item_id: detail.item.id,
        };
        for responder in self.pending.complete_accept(delivery_id).await? {
            let _ = responder.send(response.clone());
        }
        Ok(())
    }

    pub async fn reject(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        for responder in self.pending.reject(delivery_id).await? {
            let _ = responder.send(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::PolicyRejected,
            });
        }
        Ok(())
    }

    pub async fn pending(&self) -> Vec<PendingTextDeliverySummary> {
        self.pending.summaries().await
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
        let record = TextDeliveryRecord {
            delivery_id,
            direction: entity::TextDeliveryDirection::Receive,
            peer_id: from.to_string(),
            peer_name: peer
                .as_ref()
                .map(|device| device.os_info.display_name())
                .unwrap_or_else(|| from.to_string()),
            body,
            status: entity::TextDeliveryStatus::Delivered,
            failure: None,
            attempt_count: 1,
            created_at: js_time_ms(),
            updated_at: js_time_ms(),
        };
        if decision.action == ReceivePolicyAction::AutoAccept {
            return match manager.store.persist_incoming_text_delivery(record).await {
                Ok(detail) => Ok(TextDeliveryResponse::Delivered {
                    inbox_item_id: detail.item.id,
                }),
                Err(_) => Ok(TextDeliveryResponse::Rejected {
                    reason: TextDeliveryRejectReason::StorageUnavailable,
                }),
            };
        }
        let rx = match self.pending.enqueue(record).await {
            Ok(rx) => rx,
            Err(reason) => return Ok(TextDeliveryResponse::Rejected { reason }),
        };
        match n0_future::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                self.pending.expire(delivery_id).await;
                Ok(TextDeliveryResponse::Expired)
            }
        }
    }
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
        let outcome = crate::protocol::TEXT_DELIVERY
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
    use entity::{TextDeliveryDirection, TextDeliveryStatus};
    use uuid::Uuid;

    use super::{
        PendingTextDeliveries, TextDeliveryRecord, TextDeliveryRejectReason, TextDeliveryResponse,
    };

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
        let _first = queue
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
    async fn duplicate_during_confirmation_survives_storage_failure_and_gets_one_result() {
        let queue = PendingTextDeliveries::new(2);
        let delivery_id = Uuid::new_v4();
        let first = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("首次请求");
        let record = queue.begin_accept(delivery_id).await.expect("开始确认");
        assert_eq!(record.body, "body");

        // 模拟落库期间的回执丢失重发：它必须合并而不是生成第二个待确认条目。
        let retry = queue
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
        let _receiver = queue
            .enqueue(incoming(delivery_id, "peer-a", "body"))
            .await
            .expect("首次请求");
        queue.begin_accept(delivery_id).await.expect("开始确认");
        queue.expire(delivery_id).await;

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
