//! 独立文本投递 RPC 的接收端服务。

use std::sync::{Arc, Weak};
use std::time::Duration;

use dashmap::DashMap;
use swarmdrop_net::{AcceptError, Endpoint, NodeId, PathKind, RpcService};
use tokio::sync::oneshot;
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
    pending: Arc<DashMap<Uuid, PendingTextDelivery>>,
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
            pending: Arc::new(DashMap::new()),
        }
    }

    pub async fn accept(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        let (_, pending) = self
            .pending
            .remove(&delivery_id)
            .ok_or_else(|| crate::AppError::Transfer("待确认文本投递不存在或已过期".into()))?;
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| crate::AppError::Transfer("节点已停止".into()))?;
        let detail = match manager
            .store
            .persist_incoming_text_delivery(pending.record.clone())
            .await
        {
            Ok(detail) => detail,
            Err(error) => {
                // 先落库再回复是送达语义的边界；写入失败时保留原 responders，用户可修复存储后重试确认。
                self.pending.insert(delivery_id, pending);
                return Err(error);
            }
        };
        let response = TextDeliveryResponse::Delivered {
            inbox_item_id: detail.item.id,
        };
        for responder in pending.responders {
            let _ = responder.send(response.clone());
        }
        Ok(())
    }

    pub fn reject(&self, delivery_id: Uuid) -> crate::AppResult<()> {
        let (_, pending) = self
            .pending
            .remove(&delivery_id)
            .ok_or_else(|| crate::AppError::Transfer("待确认文本投递不存在或已过期".into()))?;
        for responder in pending.responders {
            let _ = responder.send(TextDeliveryResponse::Rejected {
                reason: TextDeliveryRejectReason::PolicyRejected,
            });
        }
        Ok(())
    }

    pub fn pending(&self) -> Vec<PendingTextDeliverySummary> {
        self.pending
            .iter()
            .map(|entry| PendingTextDeliverySummary {
                delivery_id: *entry.key(),
                peer_id: entry.record.peer_id.clone(),
                peer_name: entry.record.peer_name.clone(),
                body: entry.record.body.clone(),
                created_at: entry.record.created_at,
            })
            .collect()
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
        let (tx, rx) = oneshot::channel();
        match self.pending.entry(delivery_id) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                let pending = entry.get_mut();
                if pending.record.peer_id != record.peer_id || pending.record.body != record.body {
                    return Ok(TextDeliveryResponse::Rejected {
                        reason: TextDeliveryRejectReason::ProtocolConflict,
                    });
                }
                pending.responders.push(tx);
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                if self.pending.len() >= MAX_PENDING_TEXT_DELIVERIES {
                    return Ok(TextDeliveryResponse::Rejected {
                        reason: TextDeliveryRejectReason::QueueFull,
                    });
                }
                entry.insert(PendingTextDelivery {
                    record,
                    responders: vec![tx],
                });
            }
        }
        match n0_future::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                self.pending.remove(&delivery_id);
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
