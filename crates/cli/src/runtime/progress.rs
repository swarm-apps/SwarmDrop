//! 正在进行的传输的实时进度。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）。
//!
//! ## 为什么需要它：数据库里的进度在传输期间是假的
//!
//! 发送方向的进度**不是增量落库的**——`save_sender_file_progress` 只在暂停、中断、
//! 完成与续传定基线时各写一次（那是刻意的：每发一块就开一次写事务会拖慢热路径，
//! 而断点续传只需要终结时刻的那一份）。于是整条发送传输期间，
//! `list_unfinished_projections` 交出来的 `transferred_bytes` 一直是**上一次终结时的值**，
//! 通常就是 0，直到它结束才跳到全量。
//!
//! 桌面 / 移动 / Web 不受影响：它们与传输在同一个进程里，界面直接吃 `TransferProgress`
//! 事件，数据库只是「重启后的基线」。而命令行宿主的 `transfer watch` 跑在**另一个
//! 进程**里，只能问常驻节点——于是常驻节点必须自己把事件里的进度记下来，随投影一起
//! 交出去。
//!
//! ## 为什么不改成「发送侧周期落库」
//!
//! 那会给每一次传输（包括三端上的每一次）都加上周期性写事务，只为了服务一个此刻恰好
//! 有人在看的面板。缓存把代价留在需要它的那一侧：一个常驻节点、一个 `HashMap`、
//! 每条会话 16 字节。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use swarmdrop_core::host::CoreEvent;
use swarmdrop_core::transfer::store::TransferProjection;
use uuid::Uuid;

use crate::adapter::events::CliEventBus;

/// 每条**正在传**的会话的最新已传字节数。
///
/// 只保存正在传的那些：会话一进入暂停 / 终态就删掉自己那条，于是「缓存里有值」
/// 恰好等价于「此刻有个 actor 在跑，数据库那份是陈旧的」。这条不变量让
/// [`Self::overlay`] 不必再判断 phase——**判断放在写入侧只有一处，放在读取侧则
/// 每个消费者各写一遍**。
#[derive(Default)]
pub struct ProgressCache {
    latest: Mutex<HashMap<Uuid, u64>>,
}

impl ProgressCache {
    /// 起一个任务盯着事件总线。返回的缓存随节点存活。
    pub fn spawn(events: &CliEventBus) -> Arc<Self> {
        let cache = Arc::new(Self::default());
        let mut rx = events.subscribe();
        let sink = cache.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                sink.apply(&event);
            }
        });
        cache
    }

    /// 把缓存里的实时进度盖到投影上。
    ///
    /// **只盖 `transferred_bytes`，不动 `total_size`**：总量在会话建立时就定下了，
    /// 数据库那份始终是对的；而续传时事件里的总量表达的是「本轮要传多少」，
    /// 盖上去会让百分比按一个变小的分母算，进度条于是在恢复的瞬间往前跳。
    pub fn overlay(&self, projections: &mut [TransferProjection]) {
        let latest = self.latest.lock().expect("进度缓存锁中毒");
        for projection in projections {
            if let Some(&transferred) = latest.get(&projection.session_id) {
                projection.transferred_bytes = transferred as i64;
            }
        }
    }

    fn apply(&self, event: &CoreEvent) {
        match event {
            CoreEvent::TransferProgress { event } => {
                self.latest
                    .lock()
                    .expect("进度缓存锁中毒")
                    .insert(event.session_id, event.transferred_bytes);
            }
            // 五种「不再有 actor 在跑」的事件都要清掉自己那条，缺一条就会留下一个
            // 永不失效的旧值——那条会话此后每次出现在面板上都带着它最后一刻的进度，
            // 而数据库里真正的值（可能因为续传基线而变小）被它盖住。
            //
            // `TransferResumed` 也在其列：续传是**新一轮**，旧值比新基线大，
            // 不清掉的话进度条会先显示一个偏大的数，直到第一条新进度事件把它拽回来。
            CoreEvent::TransferPaused { event } => self.forget(event.session_id),
            CoreEvent::TransferResumed { event } => self.forget(event.session_id),
            CoreEvent::TransferCompleted { event } => self.forget(event.session_id),
            CoreEvent::TransferFailed { event } => self.forget(event.session_id),
            CoreEvent::TransferRejected { event } => self.forget(event.session_id),
            // 事件是个还在长的大枚举，这里刻意不逐变体匹配（同 `adapter::events`）：
            // 一个只为记进度而存在的 match 不该在每次上游加变体时变成编译错误。
            _ => {}
        }
    }

    fn forget(&self, session_id: Uuid) {
        self.latest
            .lock()
            .expect("进度缓存锁中毒")
            .remove(&session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swarmdrop_core::transfer::progress::{
        RuntimeTransferDirection, TransferPausedEvent, TransferProgressEvent,
    };

    fn progress_event(session_id: Uuid, transferred: u64) -> CoreEvent {
        CoreEvent::TransferProgress {
            event: TransferProgressEvent {
                session_id,
                direction: RuntimeTransferDirection::Send,
                total_files: 1,
                completed_files: 0,
                total_bytes: 1000,
                transferred_bytes: transferred,
                speed: 0.0,
                eta: None,
                files: Vec::new(),
            },
        }
    }

    fn projection(session_id: Uuid, stored: i64) -> TransferProjection {
        TransferProjection {
            session_id,
            direction: entity::TransferDirection::Send,
            peer_id: String::new(),
            peer_name: String::new(),
            phase: entity::TransferPhase::Active,
            suspended_reason: None,
            terminal_reason: None,
            recoverable: false,
            epoch: 0,
            total_size: 1000,
            transferred_bytes: stored,
            started_at: 0,
            updated_at: 0,
            finished_at: None,
            failure: None,
            policy_action: None,
            policy_reason: None,
            save_path: None,
            content_root: None,
            files: Vec::new(),
        }
    }

    /// **正在传的那条要用实时值盖掉库里的陈旧值。**
    ///
    /// 这条看守的正是这个模块存在的理由：发送方向的进度整条传输期间都不落库，
    /// 不盖的话 `transfer watch` 的进度条会一路停在 0%，直到传完的瞬间跳到 100%。
    #[test]
    fn a_live_session_uses_the_event_value() {
        let id = Uuid::new_v4();
        let cache = ProgressCache::default();
        cache.apply(&progress_event(id, 512));

        let mut rows = [projection(id, 0)];
        cache.overlay(&mut rows);
        assert_eq!(rows[0].transferred_bytes, 512);
        // 总量一律不动——续传时事件里的总量是「本轮要传多少」。
        assert_eq!(rows[0].total_size, 1000);
    }

    /// 缓存里没有的会话原样放过——数据库那份对它才是权威的。
    #[test]
    fn an_unknown_session_keeps_its_stored_value() {
        let cache = ProgressCache::default();
        let mut rows = [projection(Uuid::new_v4(), 777)];
        cache.overlay(&mut rows);
        assert_eq!(rows[0].transferred_bytes, 777);
    }

    /// **会话终结后必须忘掉它。**
    ///
    /// 不忘的表现很隐蔽：暂停会把真实进度写进库（可能因续传基线而**小于**最后一条
    /// 事件的值），而残留的旧值会一直盖住它——面板上那条会话此后永远显示暂停前
    /// 最后一刻的数字，且不会有任何东西纠正它。
    #[test]
    fn a_finished_session_is_forgotten() {
        let id = Uuid::new_v4();
        let cache = ProgressCache::default();
        cache.apply(&progress_event(id, 900));
        cache.apply(&CoreEvent::TransferPaused {
            event: TransferPausedEvent {
                session_id: id,
                direction: RuntimeTransferDirection::Send,
            },
        });

        let mut rows = [projection(id, 850)];
        cache.overlay(&mut rows);
        assert_eq!(rows[0].transferred_bytes, 850, "暂停后该由库里的值说了算");
    }
}
