//! [`EventBus`]：捕获 NetManager 侧的**入站配对请求**供浏览器确认（browser-as-inviter——
//! 桌面消费浏览器生成的 invite 后，浏览器作为邀请方本机弹确认）。
//!
//! Web 的 **transfer 域事件**走 [`WebEventSink`](crate::events::WebEventSink) 直连 `events()`
//! 流（不经本 bus）。本 bus 只把 [`CoreEvent::PairingRequestReceived`] 落进一个共享队列，
//! `WebNode::pending_pairing_requests` 轮询取出；其余 device/network 事件记日志不 surface。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::paired_devices;
use swarmdrop_host::{AppResult, PairedDeviceStore};

use crate::types::PendingPairingJson;

/// 挂起入站配对请求队列（WebEventBus 写、WebNode 读）。
pub type PendingPairings = Arc<Mutex<Vec<PendingPairingJson>>>;

/// 捕获入站配对请求的 EventBus。
pub struct WebEventBus {
    pending_pairings: PendingPairings,
    /// 已配对设备列表的持久化端口——只在**新增/刷新**方向用（移除方向由 core 的
    /// `unpair` 自己写完，见下面的 `PairedDeviceRemoved` 分支）。
    paired_store: Arc<dyn PairedDeviceStore>,
}

impl WebEventBus {
    /// 建 bus 与配套的共享队列句柄（后者交给 `WebNode` 供 `pending_pairing_requests` 读取）。
    pub fn new(paired_store: Arc<dyn PairedDeviceStore>) -> (Self, PendingPairings) {
        let q: PendingPairings = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                pending_pairings: q.clone(),
                paired_store,
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
            // 名字叫 Added，实际语义是「对端 identify 后刷新了已配对设备的 OS 信息」
            // （core 的 event_loop 只在 refresh_paired_device_from_identify 命中时发）。
            // 与 `WebNode::connect_invite` / `respond_pairing_request` 里那次 upsert 不重复：
            // 那是配对成功的首次写入，这里是之后的信息刷新回写。
            //
            // **这条路径不是「用户设的信任策略被重置」的成因**——`refresh_paired_device_os_info`
            // 返回的是共享 DashMap 里那条记录的 clone，`trust_level` / `receive_policy` 本来就
            // 带着正确值。成因是配对成功那两处（它们拿到的是默认策略的 `PairedDeviceInfo::new`），
            // 复现要走「对已配对设备再走一次邀请配对」，拿这里去试是试不出来的。
            CoreEvent::PairedDeviceAdded { device } => {
                let store = self.paired_store.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(err) = paired_devices::upsert(&*store, device).await {
                        tracing::warn!("持久化已配对设备失败: {err:?}");
                    }
                });
            }
            // 解除配对的通知。**这里不删持久化**：core 的 `PairingManager::unpair` 已经按
            // 「先落盘 → 再删内存表 → 再发事件」写过一遍了，再删一次虽然幂等，却会让
            // 「持久化失败」这个错误被第二次成功掩盖。
            //
            // Web 侧只记日志：设备清单走 1.5s 轮询 + 解除成功后前端主动刷新一次
            // （device 事件尚未 surface 到 JS，那是 README 里另记的一笔债）。
            CoreEvent::PairedDeviceRemoved { peer_id } => {
                tracing::info!("已解除配对: {peer_id}");
            }
            // 本机改名。**这里没有剩余动作**：落盘由 `rename_device` 的第 ① 步经
            // `IdbDeviceConfig` 做完，identify 的下发是第 ③ 步。显式列一条而不是落进下面的
            // catch-all，是想让「Web 侧真的什么都不用做」有个可读的落点——
            // 浏览器一个标签页一个节点、改名的唯一发起点就是本页 UI，而
            // `WebNode::rename_device` 把归一化后的名字直接回给调用方去更新 store，
            // 不需要桌面那种「多窗口 + MCP 也得同步」才需要的推送通道。
            CoreEvent::DeviceRenamed { name, display_name } => {
                tracing::info!("本机设备名已更新: {name:?}（对外展示 {display_name}）");
            }
            other => {
                tracing::debug!("WebEventBus core 事件（暂不 surface 到 JS）: {other:?}");
            }
        }
        Ok(())
    }
}
