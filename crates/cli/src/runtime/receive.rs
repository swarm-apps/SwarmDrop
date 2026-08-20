//! 常驻节点的被动接收。
//!
//! 接收是**节点在线时的默认行为**，不是一条命令——这与其余三端的「配对 + 被动接收」
//! 模型一致。因此常驻节点必须自己应答入站请求：没有界面可以弹确认框，等一个不会到来的
//! 人工确认，结果就是对端一直卡在「等待接受」。
//!
//! 判据是**已配对**：能发起传输的对端必然已经过配对握手，那一步才是信任边界。
//!
//! ## 两种入站内容都要应答，且理由是同一条
//!
//! 文件走 `TransferOfferReceived`，文本走 `TextDeliveryAttention`。
//! **文本那条不是补充，是必需**：新配对设备的默认信任档位（`Collaborator`）就带着
//! `require_confirmation`，于是**每一条**发给命令行宿主的文本都会落进待确认队列。
//! 没人应答的话，它在那儿躺满确认窗口（5 分钟）后过期——而发送端那条命令就阻塞这么久，
//! 最后拿到一句「对端未在确认窗口内接收」。也就是说：少了这一支，「其余三端能给 CLI
//! 发文本」这句话在默认配置下**一次都不成立**，且失败得像是网络问题。

use std::path::PathBuf;
use std::sync::Arc;

use swarmdrop_core::host::{CoreEvent, CoreSaveLocation};
use swarmdrop_core::transfer::text_delivery::TextDeliveryAttentionKind;

use super::boot::RunningNode;

/// 起一个后台任务，自动接受入站的文件与文本。
pub fn spawn_auto_accept(node: Arc<RunningNode>, save_dir: PathBuf) {
    let mut events = node.events.subscribe();

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            // 单条应答失败**不终止这个循环**：下一次入站请求仍应被处理。
            match event {
                CoreEvent::TransferOfferReceived { offer } => {
                    accept_files(&node, &offer.session_id, &save_dir).await;
                }
                // `Received` 那一种是「已经收下了」的事后通知，没有待办；
                // 只有 `ConfirmationRequired` 那一种有人在等应答。
                CoreEvent::TextDeliveryAttention { attention }
                    if attention.kind == TextDeliveryAttentionKind::ConfirmationRequired =>
                {
                    accept_text(&node, attention.delivery_id, &attention.peer_name).await;
                }
                _ => {}
            }
        }
    });
}

async fn accept_files(node: &RunningNode, session_id: &uuid::Uuid, save_dir: &std::path::Path) {
    let location = CoreSaveLocation::Path {
        path: save_dir.to_string_lossy().into_owned(),
    };

    match node
        .manager
        .transfer_arc()
        .accept_and_start_receive(session_id, location)
        .await
    {
        Ok(()) => tracing::info!(
            session = %session_id,
            "已自动接受入站传输，落点 {}",
            save_dir.display()
        ),
        Err(err) => tracing::warn!(
            session = %session_id,
            "自动接受入站传输失败: {err}"
        ),
    }
}

/// 应答一条待确认的入站文本。
///
/// 正文**不进日志**：`TextDeliveryAttention` 刻意不携带它（见 `text_delivery::attention`），
/// 而命令行宿主的日志直接落在用户终端上、也常被服务管理器收走。要看内容用
/// `swarmdrop inbox show`。
async fn accept_text(node: &RunningNode, delivery_id: uuid::Uuid, peer_name: &str) {
    let service = match node.manager.transfer_arc().text_delivery_service() {
        Ok(service) => service.clone(),
        // 组合根没装上这个服务时才会走到这里——那是装配 bug，不是运行时状况。
        Err(err) => {
            tracing::warn!(%delivery_id, "文本投递服务不可用: {err}");
            return;
        }
    };

    match service.accept(delivery_id).await {
        Ok(()) => tracing::info!(
            %delivery_id,
            "已自动接收来自 {peer_name} 的文本，用 swarmdrop inbox show 查看",
        ),
        Err(err) => tracing::warn!(%delivery_id, "自动接收入站文本失败: {err}"),
    }
}
