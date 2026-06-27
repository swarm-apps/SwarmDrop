//! transfer-data 数据面接线。
//!
//! `libs/core` 暴露通用 data channel；本模块把它路由到 SwarmDrop 的
//! SendSession / ReceiveSession，并把完成、中断映射回 DB/projection。

use std::sync::Arc;

use swarm_p2p_core::libp2p::StreamProtocol;
use swarm_p2p_core::{DataChannelReceiver, InboundDataChannel};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::host::CoreEvent;
use crate::protocol::FileRange;
use crate::transfer::coordinator::{CoordinatorInput, NetworkSignal};
use crate::transfer::data_frame::{
    TRANSFER_DATA_PROTOCOL, TransferDataFrame, TransferDataRole, read_frame, write_frame,
};
use crate::transfer::manager::TransferManager;
use crate::transfer::progress::{RuntimeTransferDirection, TransferCompleteEvent};
use crate::{AppError, AppResult};

impl TransferManager {
    pub(super) fn spawn_data_channel_task(
        self: &Arc<Self>,
        mut rx: DataChannelReceiver,
        cancel_token: CancellationToken,
    ) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        info!("transfer-data 入站任务已停止");
                        break;
                    }
                    inbound = rx.recv() => {
                        let Some(inbound) = inbound else {
                            info!("transfer-data 入站 receiver 已关闭");
                            break;
                        };
                        let manager = Arc::clone(&manager);
                        tokio::spawn(async move {
                            if let Err(e) = manager.handle_inbound_data_channel(inbound).await {
                                warn!("处理入站 transfer-data 通道失败: {}", e);
                            }
                        });
                    }
                }
            }
        });
    }

    pub(super) fn spawn_send_data_channel(
        &self,
        session_id: Uuid,
        epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) {
        let Some(session) = self.get_send_session(&session_id) else {
            warn!("启动 transfer-data 发送失败：send session 不存在: {session_id}");
            return;
        };

        let client = self.client.clone();
        let db = self.db.clone();
        let actors = self.actors.clone();
        let coordinator = self.coordinator.clone();
        let event_bus = self.event_bus.clone();
        let peer_id = session.peer_id;

        tokio::spawn(async move {
            let result = async {
                let channel = client
                    .open_data_channel(peer_id, StreamProtocol::new(TRANSFER_DATA_PROTOCOL))
                    .await
                    .map_err(|e| AppError::Transfer(format!("打开 data channel 失败: {e}")))?;
                session.run_data_channel(epoch, channel, fetch_plan).await
            }
            .await;

            match result {
                Ok(()) => {
                    actors.remove_send(&session_id);
                    if let Err(e) =
                        crate::database::ops::mark_session_completed(&db, session_id).await
                    {
                        warn!("DB 标记发送完成失败: {}", e);
                    } else {
                        let _ = coordinator.publish_projection(session_id).await;
                    }
                    let _ = event_bus
                        .publish(CoreEvent::TransferCompleted {
                            event: TransferCompleteEvent {
                                session_id,
                                direction: RuntimeTransferDirection::Send,
                                total_bytes: session.total_bytes_sent(),
                                elapsed_ms: session.elapsed_ms(),
                                save_location: None,
                            },
                        })
                        .await;
                }
                Err(e) if session.cancel_token().is_cancelled() => {
                    actors.remove_send(&session_id);
                    info!("transfer-data 发送任务已取消: session={session_id}, {e}");
                }
                Err(e) => {
                    let progress = session.get_file_progress();
                    let _ =
                        crate::database::ops::save_sender_file_progress(&db, session_id, &progress)
                            .await;
                    actors.remove_send(&session_id);
                    warn!("transfer-data 发送中断: session={session_id}, {e}");
                    let _ = coordinator
                        .dispatch(
                            session_id,
                            CoordinatorInput::Network {
                                epoch,
                                signal: NetworkSignal::Interrupted,
                            },
                        )
                        .await;
                }
            }
        });
    }

    async fn handle_inbound_data_channel(
        self: Arc<Self>,
        inbound: InboundDataChannel,
    ) -> AppResult<()> {
        if inbound.channel.protocol().as_ref() != TRANSFER_DATA_PROTOCOL {
            return Err(AppError::Transfer(format!(
                "未知 data-channel 协议: {}",
                inbound.channel.protocol()
            )));
        }

        let mut stream = inbound.channel.into_stream();
        let Some(hello) = read_frame(&mut stream).await? else {
            return Err(AppError::Transfer("data channel 未发送 Hello".into()));
        };
        let TransferDataFrame::Hello {
            session_id,
            epoch,
            role,
            manifest_digest,
            fetch_plan,
        } = hello
        else {
            return Err(AppError::Transfer("data channel 首帧不是 Hello".into()));
        };
        let Some(receive) = self.get_receive_session(&session_id) else {
            write_frame(
                &mut stream,
                &TransferDataFrame::Abort {
                    session_id,
                    epoch,
                    reason: "接收会话不存在".into(),
                },
            )
            .await?;
            return Ok(());
        };
        let Some(current_epoch) = self.actors.receive_epoch(&session_id) else {
            return Err(AppError::Transfer("接收 actor epoch 不存在".into()));
        };
        if let Err(reason) = validate_inbound_hello(
            current_epoch,
            epoch,
            role,
            receive.expected_manifest_digest(),
            manifest_digest,
            &fetch_plan,
            |plan| receive.validate_fetch_plan(plan),
        ) {
            write_frame(
                &mut stream,
                &TransferDataFrame::Abort {
                    session_id,
                    epoch,
                    reason,
                },
            )
            .await?;
            return Ok(());
        }

        let actors = self.actors.clone();
        receive.start_data_channel(epoch, stream, fetch_plan, move |sid| {
            actors.remove_receive_if_epoch(sid, epoch);
        });

        Ok(())
    }
}

fn validate_inbound_hello(
    current_epoch: i64,
    incoming_epoch: i64,
    role: TransferDataRole,
    expected_manifest_digest: [u8; 32],
    incoming_manifest_digest: [u8; 32],
    fetch_plan: &[FileRange],
    validate_fetch_plan: impl FnOnce(&[FileRange]) -> AppResult<()>,
) -> Result<(), String> {
    if role != TransferDataRole::Sender {
        return Err("接收方只接受 Sender Hello".into());
    }
    if current_epoch != incoming_epoch {
        return Err(format!(
            "旧 epoch: current={current_epoch}, incoming={incoming_epoch}"
        ));
    }
    if expected_manifest_digest != incoming_manifest_digest {
        return Err("manifest digest 不匹配".into());
    }
    validate_fetch_plan(fetch_plan).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::protocol::FileInfo;
    use crate::transfer::data_frame::{full_fetch_plan, manifest_digest};

    use super::*;

    fn manifest() -> Vec<FileInfo> {
        vec![FileInfo {
            file_id: 1,
            name: "a.txt".into(),
            relative_path: "a.txt".into(),
            size: 8,
            checksum: "checksum".into(),
        }]
    }

    #[test]
    fn inbound_hello_rejects_old_epoch() {
        let manifest = manifest();
        let digest = manifest_digest(&manifest);

        let err = validate_inbound_hello(
            3,
            2,
            TransferDataRole::Sender,
            digest,
            digest,
            &full_fetch_plan(&manifest),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(err.contains("旧 epoch"));
    }

    #[test]
    fn inbound_hello_rejects_manifest_mismatch() {
        let manifest = manifest();
        let expected = manifest_digest(&manifest);
        let incoming = manifest_digest(&[FileInfo {
            checksum: "changed".into(),
            ..manifest[0].clone()
        }]);

        let err = validate_inbound_hello(
            1,
            1,
            TransferDataRole::Sender,
            expected,
            incoming,
            &full_fetch_plan(&manifest),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(err.contains("manifest"));
    }
}
