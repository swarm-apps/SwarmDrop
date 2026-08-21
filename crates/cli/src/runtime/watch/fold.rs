//! 把原始事件流收敛成真正该发出去的那些帧。
//!
//! **纯逻辑，零 IO、零时钟**：什么时候到降频窗口由调用方的计时器说了算，本模块只回答
//! 「这条现在该发吗」和「攒着的那些是什么」。这让两条最容易写错的规则可以被直接单测。
//!
//! ## 两条收敛规则，各有各的失败形态
//!
//! - **采样类按会话 last-value-wins**。进度在领域侧是 200ms 一帧，逐帧转发出去的是
//!   一条谁也不会去读、却要被消费方逐条持久化的流水账。攒最新的那帧、每秒交一次。
//! - **设备表按内容去重**。内核的 `DevicesChanged` 由网络事件驱动，每次 ping 成功都推
//!   一遍**全量**——绝大多数与上一遍逐字相同。
//!
//! ## 去抖为什么是「按内容」而不是「按时间窗」
//!
//! 时间窗会**延迟**一条边沿事件，而边沿事件不得丢弃也不该迟到（用户刚配上对，订阅要
//! 立刻看得见）。设备事件的高频来源本来就不是真的翻转，是内容相同的重复——按内容去重
//! 把它们全部吸收，同时一次真正的上下线一帧不差地立刻发出去。

use std::collections::BTreeMap;
use std::time::Duration;

use super::event::{DeviceEntry, ProgressSample, WatchEvent};
use crate::runtime::transfers::PHASE_TERMINAL;

/// 采样类事件的降频窗口。
///
/// 比领域侧的节流（200ms）粗一档是刻意的：这条流的消费方会**逐条持久化**，
/// 一次几万文件的传输若按原频率转发，产出的是数万条记录（spec: `cli-event-stream`
/// 的「进度是聚合的、降频的」）。
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// 收敛器。
#[derive(Debug, Default)]
pub struct Coalescer {
    /// 每条会话攒着的最新一帧进度，等下一次降频窗口到点再交出去。
    ///
    /// `BTreeMap` 不是随手挑的：交出去的次序必须是确定的，否则同一批输入在两次运行里
    /// 产出不同的行序，而这条流会被持久化并比对。
    pending: BTreeMap<String, ProgressSample>,
    /// 上一次交给消费方的设备表。`None` = 还没有任何一份到过对面。
    devices: Option<Vec<DeviceEntry>>,
}

impl Coalescer {
    /// 以一份**已经随基线交出去过**的设备表开局。
    ///
    /// 不是优化：基线里就带着这张表，而节点起来后第一条 `DevicesChanged` 几乎必然与它
    /// 逐字相同。从 [`Default`] 开局的话，消费方会在基线之后立刻再收到一条内容完全一样
    /// 的变化事件——它不得不去比对才发现什么都没变，而这条流上「收到变化事件」本该
    /// 就意味着变了。
    pub fn seeded(devices: Vec<DeviceEntry>) -> Self {
        Self {
            devices: Some(devices),
            ..Self::default()
        }
    }

    /// 收下一条翻译后的事件。
    ///
    /// 返回 `None` = 它被吸收了：要么攒进降频窗口（采样类），要么与上一次发出去的
    /// 逐字相同（设备表）。其余一律**立刻**原样交出——边沿事件不得延迟。
    pub fn accept(&mut self, event: WatchEvent) -> Option<WatchEvent> {
        match event {
            WatchEvent::TransferProgress(sample) => {
                self.pending.insert(sample.session_id.clone(), sample);
                None
            }
            WatchEvent::DevicesChanged { devices } => {
                if self.devices.as_ref() == Some(&devices) {
                    return None;
                }
                self.devices = Some(devices.clone());
                Some(WatchEvent::DevicesChanged { devices })
            }
            // 会话走到终态时把攒着的那帧丢掉。
            //
            // 不丢的话它会在终态之后再冒出来一帧——消费方按事件顺序重建时，
            // 一条已经结束的传输会退回「传输中」。
            WatchEvent::TransferChanged(entry) if entry.phase == PHASE_TERMINAL => {
                self.pending.remove(&entry.session_id);
                Some(WatchEvent::TransferChanged(entry))
            }
            other => Some(other),
        }
    }

    /// 降频窗口到点：把攒着的采样交出来。
    pub fn flush(&mut self) -> Vec<WatchEvent> {
        std::mem::take(&mut self.pending)
            .into_values()
            .map(WatchEvent::TransferProgress)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::transfers::{PHASE_ACTIVE, PHASE_TERMINAL};

    fn sample(session: &str, transferred: i64) -> WatchEvent {
        WatchEvent::TransferProgress(ProgressSample {
            session_id: session.into(),
            direction: "receive".into(),
            transferred_bytes: transferred,
            total_bytes: 100,
            completed_files: 0,
            total_files: 1,
            speed: transferred as f64,
            eta: None,
        })
    }

    fn device(peer_id: &str, online: bool) -> DeviceEntry {
        DeviceEntry {
            peer_id: peer_id.into(),
            name: "手机".into(),
            online: Some(online),
        }
    }

    fn transfer(session: &str, phase: &str) -> WatchEvent {
        WatchEvent::TransferChanged(super::super::event::TransferEntry {
            session_id: session.into(),
            direction: "receive".into(),
            peer_name: "手机".into(),
            phase: phase.into(),
            suspended_reason: None,
            terminal_reason: None,
            recoverable: false,
            transferred_bytes: 0,
            total_bytes: 100,
            file_count: 1,
            failure: None,
            updated_at: 0,
        })
    }

    /// **一秒内的密集进度只交出最新的那一帧**（spec: `cli-event-stream` 的
    /// 「同一会话的密集进度」）。
    #[test]
    fn dense_progress_collapses_to_the_latest_frame() {
        let mut fold = Coalescer::default();
        for n in 1..=5 {
            assert!(
                fold.accept(sample("a", n * 10)).is_none(),
                "采样不得立刻发出"
            );
        }

        let flushed = fold.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0], sample("a", 50));
        assert!(fold.flush().is_empty(), "交出去之后不该还留着");
    }

    /// 折叠是**按会话**的——两条会话同时在传时谁也不该盖掉谁。
    #[test]
    fn sessions_fold_independently() {
        let mut fold = Coalescer::default();
        fold.accept(sample("a", 10));
        fold.accept(sample("b", 20));
        fold.accept(sample("a", 30));

        assert_eq!(fold.flush(), vec![sample("a", 30), sample("b", 20)]);
    }

    /// 边沿事件**立刻**交出，不进降频窗口。
    #[test]
    fn edge_events_pass_through_immediately() {
        let mut fold = Coalescer::default();
        let event = WatchEvent::InboxRemoved {
            item_id: "x".into(),
        };
        assert_eq!(fold.accept(event.clone()), Some(event));
    }

    /// **基线里已经交出去过的那份不该再发一遍。**
    ///
    /// 节点起来后第一条 `DevicesChanged` 几乎必然与基线里那张表逐字相同；不 seed 的话
    /// 消费方会在基线之后立刻收到一条什么都没变的「变化事件」。
    #[test]
    fn the_table_carried_by_the_baseline_is_not_repeated() {
        let table = vec![device("A", true)];
        let mut fold = Coalescer::seeded(table.clone());

        assert!(
            fold.accept(WatchEvent::DevicesChanged { devices: table })
                .is_none(),
            "基线已经带过这张表了"
        );
    }

    /// **内容相同的设备表只发一次**——内核每次 ping 成功都推一遍全量。
    #[test]
    fn an_unchanged_device_table_is_absorbed() {
        let mut fold = Coalescer::default();
        let table = WatchEvent::DevicesChanged {
            devices: vec![device("A", true)],
        };

        assert_eq!(fold.accept(table.clone()), Some(table.clone()));
        assert!(
            fold.accept(table.clone()).is_none(),
            "重复的全量表不该再发一遍"
        );
        assert!(fold.accept(table).is_none());
    }

    /// 真正的翻转必须**立刻**发出，一帧不差——它是边沿事件。
    #[test]
    fn a_real_status_flip_is_reported_at_once() {
        let mut fold = Coalescer::default();
        fold.accept(WatchEvent::DevicesChanged {
            devices: vec![device("A", true)],
        });

        let flipped = WatchEvent::DevicesChanged {
            devices: vec![device("A", false)],
        };
        assert_eq!(fold.accept(flipped.clone()), Some(flipped));
    }

    /// **终态之后不得再冒出一帧进度。**
    ///
    /// 攒着的采样若在终态之后才交出去，消费方按事件顺序重建时会看到一条已经结束的
    /// 传输退回「传输中」——而它把这段记录长期留存，事后无从分辨是真的重试还是错序。
    #[test]
    fn a_terminal_phase_discards_the_pending_sample() {
        let mut fold = Coalescer::default();
        fold.accept(sample("a", 10));
        fold.accept(transfer("a", PHASE_TERMINAL));

        assert!(fold.flush().is_empty(), "终态之后不该还留着这条会话的进度");
    }

    /// 非终态的阶段变化不动攒着的进度——那条会话还在传。
    #[test]
    fn a_non_terminal_phase_keeps_the_pending_sample() {
        let mut fold = Coalescer::default();
        fold.accept(sample("a", 10));
        fold.accept(transfer("a", PHASE_ACTIVE));

        assert_eq!(fold.flush().len(), 1);
    }
}
