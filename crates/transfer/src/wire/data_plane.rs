//! transfer-data 数据面接线。
//!
//! 新内核按协议路由裸流（[`P2pStream`]）；本模块把它路由到 SwarmDrop 的
//! SenderActor / ReceiverActor 并做注册表簿记。入站由 [`TransferDataHandler`]
//! （实现 [`ProtocolHandler`]）驱动，出站由 [`Endpoint::open`] 打开。终态副作用
//! （完成 / 中断映射回 DB/projection）下沉到 actor 自身的 `on_completed` /
//! `on_interrupted` 与 `finish_data_channel` / `fail_session`，本模块只做纯路由。

use std::sync::Arc;

use swarmdrop_net::{AcceptError, Endpoint, NodeId, OpenError, P2pStream, ProtocolHandler};
use tracing::{info, warn};
use uuid::Uuid;

use crate::coordinator::{ActorReport, CoordinatorInput};
use crate::epoch::EpochGuard;
use crate::manager::TransferManager;
use crate::progress::{RuntimeTransferDirection, TransferFailedEvent};
use crate::protocol::{FileRange, TRANSFER_DATA_PROTOCOL};
use crate::wire::data_frame::{TransferDataFrame, TransferDataRole, read_frame, write_frame};
use crate::{AppError, AppResult};

/// transfer-data 数据面协议处理器（注册进 Router）。
///
/// 每条入站数据面流在独立任务上调用 [`accept`](ProtocolHandler::accept)：读 Hello、
/// 校验归属与 epoch/manifest，再把流交给对应 ReceiverActor。
#[derive(Clone)]
pub struct TransferDataHandler {
    manager: Arc<TransferManager>,
}

impl std::fmt::Debug for TransferDataHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferDataHandler")
            .finish_non_exhaustive()
    }
}

impl TransferDataHandler {
    pub fn new(manager: Arc<TransferManager>) -> Self {
        Self { manager }
    }
}

impl ProtocolHandler for TransferDataHandler {
    async fn accept(&self, stream: P2pStream) -> Result<(), AcceptError> {
        self.manager
            .clone()
            .handle_inbound_data_stream(stream)
            .await
            .map_err(AcceptError::from_err)
    }
}

/// 打开数据面流失败的两类结局。**分类必须在这里做**——再往上一层错误就被压成
/// `AppError::Transfer(String)`，谁也分不出「重试有用」和「重试一万次也没用」。
enum OpenFailure {
    /// 对端不认识本机的数据面协议名：**版本不兼容**。
    ProtocolUnsupported,
    /// 拨号超时、连接被拒、对端下线——重试有意义。
    Transient(AppError),
}

/// 打开数据面流，只拨 [`TRANSFER_DATA_PROTOCOL`]。
///
/// **没有向旧协议名的回退**。曾经有过一条（`/3` 被拒退回 `/2`），随 chunk group 变更
/// 一并摘除：留着旧名却跑新的验签树形状，等于把「版本不匹配」伪装成「传输老是断」。
///
/// 摘除只完成了一半——`UnsupportedProtocol` 还得**被分类**。它一度和拨号超时一样压成
/// 一句字符串，于是版本不兼容走 `Interrupted` → suspended/recoverable，续传机器拿同一个
/// 协议名一次次重试：协商阶段确实响亮地失败了，但那份信息在函数里就没了，用户看到的
/// 仍然是「传输老是断」。把不兼容前移到协商阶段是 bump 协议名的全部价值，而价值只有在
/// 它到达用户时才兑现。
async fn open_data_stream(endpoint: &Endpoint, peer: NodeId) -> Result<P2pStream, OpenFailure> {
    endpoint
        .open(peer, TRANSFER_DATA_PROTOCOL)
        .await
        .map_err(|e| match e {
            OpenError::UnsupportedProtocol(_) => OpenFailure::ProtocolUnsupported,
            other => OpenFailure::Transient(AppError::Transfer(format!(
                "打开 data channel 失败: {other}"
            ))),
        })
}

impl TransferManager {
    /// 打开出站数据面流，绑定发送会话并按 fetch_plan 连续推送。
    pub(crate) fn spawn_send_data_channel(
        &self,
        session_id: Uuid,
        epoch: i64,
        fetch_plan: Vec<FileRange>,
    ) {
        let Some(session) = self.get_send_actor(&session_id) else {
            warn!("启动 transfer-data 发送失败：send session 不存在: {session_id}");
            return;
        };

        let endpoint = self.endpoint.clone();
        let store = self.store.clone();
        let actors = self.actors.clone();
        let coordinator = self.coordinator.clone();
        let events = self.events.clone();
        let peer_id = session.peer_id;

        n0_future::task::spawn(async move {
            let opened = open_data_stream(&endpoint, peer_id).await;

            // data_plane 只做路由 + 注册表簿记；终态副作用（dispatch / 落库 / 完成事件）
            // 下沉到 SenderActor::on_completed / on_interrupted（与接收方对称）。
            // 按 epoch 移除：旧 epoch 的收尾任务不得误删 resume 后注册的新 epoch sender
            // （与接收方 start_data_channel 的 remove_receive_if_epoch 对称）。
            //
            // 版本不兼容是**唯一**不走那两条收尾的分支：它不可恢复，续传只会拿同一个
            // 协议名再撞一次，所以直接推终态并给出「有一端需要升级」这个判别码。
            if let Err(OpenFailure::ProtocolUnsupported) = &opened {
                warn!(
                    "对端不支持 {TRANSFER_DATA_PROTOCOL}，判定版本不兼容: session={session_id}, peer={peer_id}"
                );
                actors.remove_send_if_epoch(&session_id, epoch);
                // **必须通知对端**：控制面协议没变，对端已经在 Offer 那一步接受了这次
                // 传输、正等着数据面连进来。本端推终态它是看不见的——而接收侧只有**启动时**
                // 回收过期会话（`stale-receive-session-expiry`），没有运行期空闲超时，
                // 所以不发这一帧，对端那条会话会一直挂到下次重启且超过保留期。
                crate::flow::send::notify_cancel_on(
                    &endpoint,
                    peer_id,
                    session_id,
                    "对方的 SwarmDrop 版本不支持当前的传输协议",
                )
                .await;
                if let Err(e) = coordinator
                    .dispatch(
                        session_id,
                        CoordinatorInput::Actor {
                            epoch,
                            report: ActorReport::FatalError(
                                crate::failure::FailureCode::PeerProtocolUnsupported,
                            ),
                        },
                    )
                    .await
                {
                    warn!("dispatch 协议不兼容 fatal 失败: session={session_id}, {e}");
                }
                return;
            }

            let result = match opened {
                Ok(stream) => session.run_data_channel(epoch, stream, fetch_plan).await,
                Err(OpenFailure::Transient(e)) => Err(e),
                // 上面那条 `if let` 已经 return，这里不可达；写成 Transient 的同义分支
                // 而不是 unreachable!()，免得将来加分支时把 panic 留在数据面上。
                Err(OpenFailure::ProtocolUnsupported) => {
                    Err(AppError::Transfer("对端不支持当前数据面协议".into()))
                }
            };

            actors.remove_send_if_epoch(&session_id, epoch);
            match result {
                Ok(()) => {
                    session
                        .on_completed(epoch, coordinator.as_ref(), store.as_ref())
                        .await;
                }
                Err(e) if session.cancel_token().is_cancelled() => {
                    info!("transfer-data 发送任务已取消: session={session_id}, {e}");
                }
                Err(e) => {
                    warn!("transfer-data 发送中断: session={session_id}, {e}");
                    // Interrupted 是可恢复状态，projection 仍是前端的权威状态；但它不保存
                    // 底层拨号/流错误。额外发出失败事件仅用于呈现这次中断的具体原因，避免
                    // Web 端只能看到笼统的 suspended/interrupted 而无法诊断。
                    if let Err(event_error) = events
                        .emit(crate::events::TransferEvent::TransferFailed {
                            event: TransferFailedEvent {
                                session_id,
                                direction: RuntimeTransferDirection::Send,
                                error: e.to_string(),
                            },
                        })
                        .await
                    {
                        warn!(
                            "上报发送失败事件失败: session={}, error={}",
                            session_id, event_error
                        );
                    }
                    session
                        .on_interrupted(epoch, coordinator.as_ref(), store.as_ref())
                        .await;
                }
            }
        });
    }

    /// 处理一条入站数据面裸流：读 Hello → 校验归属/epoch/manifest → 交 ReceiverActor。
    async fn handle_inbound_data_stream(self: Arc<Self>, mut stream: P2pStream) -> AppResult<()> {
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
        let Some(receive) = self.get_receive_actor(&session_id) else {
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

        // 归属校验：传输层身份即归属证明（取代已删除的应用层加密所隐式承担的归属校验）。
        // 流的远端必须与会话记录的发送方一致，不匹配立即断流（不发 Abort，不泄露）。
        if stream.remote() != receive.peer_id {
            return Err(AppError::Transfer(format!(
                "data stream 归属校验失败: session={session_id}, remote={}, expected={}",
                stream.remote(),
                receive.peer_id
            )));
        }

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

        // **在当前 handler 任务内 await 到完成，不 spawn**（wasm lost-wakeup 修复）：流不得跨任务
        // move，否则 muxer 后续帧的 wake 打给旧 waker、新任务永久 Pending。Router 的 per-stream
        // 任务本就设计为可长跑，accept 返回即流生命周期结束（iroh「形状 A：在 accept 里跑完」）。
        let actors = self.actors.clone();
        receive
            .start_data_channel(epoch, stream, fetch_plan, move |sid| {
                actors.remove_receive_if_epoch(sid, epoch);
            })
            .await?;

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
    if !EpochGuard::matches(incoming_epoch, current_epoch) {
        return Err(format!(
            "epoch 不匹配: current={current_epoch}, incoming={incoming_epoch}"
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
    use crate::wire::data_frame::{full_fetch_plan, manifest_digest};

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

        assert!(err.contains("epoch 不匹配"));
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
