//! 常驻节点的被动接收。
//!
//! 接收是**节点在线时的默认行为**，不是一条命令——这与其余三端的「配对 + 被动接收」
//! 模型一致。因此常驻节点必须自己应答入站请求：没有界面可以弹确认框，等一个不会到来的
//! 人工确认，结果就是对端一直卡在「等待接受」。
//!
//! 判据是**已配对**：能发起传输的对端必然已经过配对握手，那一步才是信任边界。

use std::path::PathBuf;
use std::sync::Arc;

use swarmdrop_core::host::{CoreEvent, CoreSaveLocation};

use super::boot::RunningNode;

/// 起一个后台任务，自动接受入站传输。
pub fn spawn_auto_accept(node: Arc<RunningNode>, save_dir: PathBuf) {
    let mut events = node.events.subscribe();

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let CoreEvent::TransferOfferReceived { offer } = event else {
                continue;
            };

            let location = CoreSaveLocation::Path {
                path: save_dir.to_string_lossy().into_owned(),
            };

            match node
                .manager
                .transfer_arc()
                .accept_and_start_receive(&offer.session_id, location)
                .await
            {
                Ok(()) => tracing::info!(
                    session = %offer.session_id,
                    "已自动接受入站传输，落点 {}",
                    save_dir.display()
                ),
                // 接受失败不该终止这个循环：下一次入站请求仍应被处理。
                Err(err) => tracing::warn!(
                    session = %offer.session_id,
                    "自动接受入站传输失败: {err}"
                ),
            }
        }
    });
}
