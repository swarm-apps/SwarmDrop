//! [`EventBus`]：捕获 NetManager 侧的**入站配对请求**供浏览器确认（browser-as-inviter——
//! 桌面消费浏览器生成的 invite 后，浏览器作为邀请方本机弹确认）。
//!
//! Web 的 **transfer 域事件**走 [`WebEventSink`](crate::events::WebEventSink) 直连 `events()`
//! 流（不经本 bus）。本 bus 只把 [`CoreEvent::PairingRequestReceived`] 落进一个共享队列，
//! `WebNode::pending_pairing_requests` 轮询取出；其余 device/network 事件记日志不 surface。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_host::AppResult;

use crate::types::PendingPairingJson;

/// 挂起入站配对请求队列（WebEventBus 写、WebNode 读）。
pub type PendingPairings = Arc<Mutex<Vec<PendingPairingJson>>>;

/// 捕获入站配对请求的 EventBus。
pub struct WebEventBus {
    pending_pairings: PendingPairings,
}

impl WebEventBus {
    /// 建 bus 与配套的共享队列句柄（后者交给 `WebNode` 供 `pending_pairing_requests` 读取）。
    pub fn new() -> (Self, PendingPairings) {
        let q: PendingPairings = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                pending_pairings: q.clone(),
            },
            q,
        )
    }
}

#[async_trait]
impl EventBus for WebEventBus {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        match event {
            CoreEvent::PairingRequestReceived {
                peer_id,
                pending_id,
                request,
            } => {
                let device_name = request.os_info.display_name();
                // 无 await，MutexGuard 不跨 await——publish 的 Send future 约束满足。
                if let Ok(mut q) = self.pending_pairings.lock() {
                    q.push(PendingPairingJson {
                        pending_id: pending_id.to_string(),
                        peer_id: peer_id.to_string(),
                        device_name,
                    });
                }
            }
            other => {
                tracing::debug!("WebEventBus core 事件（暂不 surface 到 JS）: {other:?}");
            }
        }
        Ok(())
    }
}
