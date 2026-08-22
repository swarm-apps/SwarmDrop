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
//! 每条会话几十个字节。
//!
//! ## 速率也在这里，而且**不能由客户端自己算**
//!
//! 面板上的速率与剩余时间同样只存在于事件里（核心的 3 秒滑窗）。客户端手上只有一串
//! 每秒一次的快照，照着它反推速率就要维护一个自己的时间窗口——`transfer watch` 第一版
//! 干脆把这件事交给了 indicatif 的估算器，于是同一次传输在手机上显示 20 MiB/s、在这边
//! 显示 200 MiB/s。判据见 [`crate::render::send::rate_and_eta`]。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use swarmdrop_core::host::CoreEvent;
use swarmdrop_core::transfer::store::TransferProjection;
use uuid::Uuid;

use crate::adapter::events::CliEventBus;

/// 一条正在传的会话的最新一帧里，面板用得上的那几个数。
///
/// **速率与剩余时间必须与已传字节数同源**（同一帧、同一把锁），否则面板会画出
/// 「进度在动、速率是 —」或反过来的自相矛盾的一屏。核心那侧为同一条理由让
/// `speed` 与 `eta` 在一帧里同源，缓存不该在跨进程这一段把它拆开。
struct Live {
    transferred: u64,
    /// B/s。核心算的 3 秒滑窗值，**本 crate 不重算**——判据见
    /// [`crate::render::send::rate_and_eta`]。
    speed: f64,
    /// 剩余秒数；核心算不出来时是 `None`。
    eta: Option<f64>,
}

/// 每条**正在传**的会话的最新一帧进度。
///
/// 只保存正在传的那些：会话一进入暂停 / 终态就删掉自己那条，于是「缓存里有值」
/// 恰好等价于「此刻有个 actor 在跑，数据库那份是陈旧的」。这条不变量让
/// [`Self::overlay`] 不必再判断 phase——**判断放在写入侧只有一处，放在读取侧则
/// 每个消费者各写一遍**。
#[derive(Default)]
pub struct ProgressCache {
    latest: Mutex<HashMap<Uuid, Live>>,
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
            if let Some(live) = latest.get(&projection.session_id) {
                projection.transferred_bytes = live.transferred as i64;
            }
        }
    }

    /// 把实时的速率与剩余时间标注到**已经 JSON 化**的那份记录上。
    ///
    /// 与 [`Self::overlay`] 分成两步而不是合并，是因为这两个数在 `TransferProjection`
    /// 里**没有对应字段、也不该有**：那是存储的投影，而速率是只在传输进行时存在的派生量
    /// ——给它加一列等于让每条历史记录都带一个恒为空的字段，还要为它写一次迁移。
    ///
    /// 只标注缓存里有的那几条。没有 actor 在跑就没有速率，此时**字段缺席**与
    /// 「不知道」是同一件事，面板会照 [`crate::render::send::rate_and_eta`] 的规则
    /// 画占位符——那比标一个 0 诚实（0 B/s 是一个断言）。
    pub fn annotate(&self, records: &mut serde_json::Value) {
        // 单条详情（`TransferShow`）与清单（`TransferUnfinished`）走同一个标注器：
        // 两者是同一个投影的一条与多条，而「只有清单带速率」会让 `transfer show`
        // 成为唯一说不出速率的读面——外部宿主拿它做「查一条传输的状态」时，得到的
        // 是一个恒空的字段，比没有这个字段更误导。
        let rows: &mut [serde_json::Value] = match records {
            serde_json::Value::Array(rows) => rows.as_mut_slice(),
            row @ serde_json::Value::Object(_) => std::slice::from_mut(row),
            _ => return,
        };
        let latest = self.latest.lock().expect("进度缓存锁中毒");
        for row in rows {
            let Some(live) = row
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
                .and_then(|id| latest.get(&id))
            else {
                continue;
            };
            let Some(object) = row.as_object_mut() else {
                continue;
            };
            object.insert("speed".into(), serde_json::json!(live.speed));
            // 算不出来时写 `null` 而不是省略：两者对读取方是同一件事，但显式的 `null`
            // 让「节点认得这个字段、只是此刻没有值」看得出来。
            object.insert("eta".into(), serde_json::json!(live.eta));
        }
    }

    fn apply(&self, event: &CoreEvent) {
        match event {
            CoreEvent::TransferProgress { event } => {
                self.latest.lock().expect("进度缓存锁中毒").insert(
                    event.session_id,
                    Live {
                        transferred: event.transferred_bytes,
                        speed: event.speed,
                        eta: event.eta,
                    },
                );
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
        rated_event(session_id, transferred, 0.0, None)
    }

    fn rated_event(session_id: Uuid, transferred: u64, speed: f64, eta: Option<f64>) -> CoreEvent {
        CoreEvent::TransferProgress {
            event: TransferProgressEvent {
                session_id,
                direction: RuntimeTransferDirection::Send,
                total_files: 1,
                completed_files: 0,
                total_bytes: 1000,
                transferred_bytes: transferred,
                speed,
                eta,
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

    /// **速率必须随记录一起过通道。**
    ///
    /// 少了这一步，面板只剩一串每秒一次的快照，只能自己估——而那会高出一个数量级
    /// （判据见 `render::send::rate_and_eta`）。
    #[test]
    fn a_live_session_carries_its_rate_across_the_channel() {
        let id = Uuid::new_v4();
        let cache = ProgressCache::default();
        cache.apply(&rated_event(id, 512, 2048.0, Some(30.0)));

        let mut records = serde_json::json!([{ "sessionId": id.to_string() }]);
        cache.annotate(&mut records);
        assert_eq!(records[0]["speed"], 2048.0);
        assert_eq!(records[0]["eta"], 30.0);
    }

    /// 缓存里没有的会话**一个字段都不加**——没有 actor 在跑就没有速率可言，
    /// 而字段缺席正是渲染侧「说不出来」的入口。
    #[test]
    fn an_unknown_session_gets_no_rate_fields() {
        let cache = ProgressCache::default();
        let mut records = serde_json::json!([{ "sessionId": Uuid::new_v4().to_string() }]);
        cache.annotate(&mut records);
        assert!(records[0].get("speed").is_none());
        assert!(records[0].get("eta").is_none());
    }

    /// **单条详情与清单走同一个标注器。**
    ///
    /// `transfer show` 返回的是一个对象而不是数组。只认数组的话，那条读面会成为
    /// 唯一说不出速率的一个——而它恰恰是「查一条传输现在怎么样」的入口。
    #[test]
    fn a_single_record_is_annotated_too() {
        let id = Uuid::new_v4();
        let cache = ProgressCache::default();
        cache.apply(&rated_event(id, 512, 2048.0, Some(30.0)));

        let mut record = serde_json::json!({ "sessionId": id.to_string() });
        cache.annotate(&mut record);
        assert_eq!(record["speed"], 2048.0);
        assert_eq!(record["eta"], 30.0);
    }

    /// 核心算不出 ETA 时写 `null`，不折成 0——0 秒是「马上就好」，那是另一件事。
    #[test]
    fn an_unknown_eta_stays_null() {
        let id = Uuid::new_v4();
        let cache = ProgressCache::default();
        cache.apply(&rated_event(id, 512, 2048.0, None));

        let mut records = serde_json::json!([{ "sessionId": id.to_string() }]);
        cache.annotate(&mut records);
        assert!(records[0]["eta"].is_null());
    }
}
