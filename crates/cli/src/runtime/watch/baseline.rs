//! 基线：订阅建立、以及每次接上常驻节点时的那一条整值快照。
//!
//! ## 为什么两条来源共用一份装配
//!
//! 订阅**不启动节点**，于是同一条基线有两种拼法：常驻节点自己拼（在线状态是真的探测
//! 结果，进度要盖实时值），或本进程直读本机记录（没有节点也就没有探测与正在传的会话）。
//!
//! 差别全部由 [`Source`] 表达，不由两份实现表达——两份实现意味着「基线里有哪几样东西」
//! 有两个答案，而消费方拿到哪一个取决于它订阅那一刻本机恰好有没有节点在跑。
//!
//! ## 为什么基线只发最近 N 条收件箱
//!
//! 条目会累积到数千条，而消费方（把基线转写进 agent 会话日志的插件）为每次订阅都搬运
//! 一次全量既昂贵又无用。真正需要更早的条目时，按需检索才是正确的取数方式
//! （spec: `cli-event-stream` 的「初始基线」）。
//!
//! ⚠️ **N 只截了「发出去多少」，没有截「读回来多少」。** `list_inbox_items` 的端口契约是
//! 「排除软删项、按 `received_at` 倒序」，没有条数参数——所以每拼一次基线仍然要把整张表
//! 连同文件行读回内存，再扔掉第 N 条之后的部分。
//!
//! 这条已知代价没有在本次改动里消除，理由是它**不是本命令引入的**：`swarmdrop inbox list`
//! 每次执行都在做同一件事。真要收口得给端口补一个带上限的列表方法（体例现成：
//! `search_inbox_capped`），而那要同时改 SQL 与 IndexedDB 两个实现——属于收件箱端口自己
//! 的一次改动，不该搭在订阅面这班车上。
//!
//! 在此之前，让它不至于变成问题的是**重连退避**（见 [`super::client`]）：拼基线只发生在
//! 订阅建立与每次接上节点时，而不是一个可以满速重试的循环里。

use swarmdrop_core::transfer::store::TransferStore;

use crate::exit::CliResult;
use crate::runtime::progress::ProgressCache;

use super::event::{Baseline, DeviceEntry, InboxEntry, TransferEntry};

/// 基线里收件箱条目的默认条数上限。
///
/// 挑 50 的依据与 `INBOX_SEARCH_LIMIT` 同源：一屏之内、一次转写之内够用，
/// 而超出的部分本来就该按需检索。
pub const DEFAULT_INBOX_LIMIT: u32 = 50;

/// 基线的取数来源。
///
/// 它同时决定 [`Baseline::node_running`]——**不另收一个布尔参数**：两个必须彼此一致的
/// 参数迟早会有一次不一致，而那次不一致不报错，只是让消费方按「节点在跑」去解读一份
/// 全是 `null` 的在线状态。
pub enum Source<'a> {
    /// 常驻节点自己拼。
    Node {
        devices: Vec<DeviceEntry>,
        /// 正在传的那几条的实时进度。
        ///
        /// **必须盖**：发送方向的进度整条传输期间都不落库，直连库读到的是上一次终结时
        /// 的值（通常是 0）。不盖的症状是「进度一路停在 0%，暂停的瞬间跳到 43%」——
        /// `transfer watch` 第一版栽过。判据见 [`ProgressCache`]。
        progress: &'a ProgressCache,
    },
    /// 没有常驻节点时本进程直读本机记录。
    ///
    /// 在线状态未知（没做过探测），库里的进度就是最新的——没有节点就没有正在跑的 actor，
    /// 也就没有「库里那份是陈旧的」这回事。
    Records { devices: Vec<DeviceEntry> },
}

/// 拼一条基线。
pub async fn build(
    store: &dyn TransferStore,
    source: Source<'_>,
    inbox_limit: u32,
) -> CliResult<Baseline> {
    let mut transfers = crate::runtime::transfers::unfinished(store).await?;
    let node_running = matches!(source, Source::Node { .. });
    // 按值解构：设备表直接搬走，不留一次白克隆。
    let devices = match source {
        Source::Node { devices, progress } => {
            progress.overlay(&mut transfers);
            devices
        }
        Source::Records { devices } => devices,
    };

    // 端口契约已保证按 `received_at` 倒序，所以「最近 N 条」就是取前 N 条——
    // **不在这里重排**，再排一次只会掩盖端口实现违约的情形。
    //
    // `include_archived: false`：基线回答的是「此刻手边有什么可用」，而归档正是
    // 「我把它收起来了」的表达（判据见 `event::InboxEntry`）。
    let items = crate::runtime::inbox::list(store, false).await?;
    let (recent, has_more) = most_recent(&items, inbox_limit as usize);

    Ok(Baseline {
        inbox: recent.iter().map(InboxEntry::from).collect(),
        inbox_has_more: has_more,
        devices,
        transfers: transfers.iter().map(TransferEntry::from).collect(),
        node_running,
    })
}

/// 取最近 N 条，并说明还有没有更早的。
///
/// 单独成函数是为了能被直接测到。这里的 off-by-one 不会让任何东西报错：恰好 N 条时
/// 多说一句「还有更早的」，消费方就会去做一次永远查不到东西的检索，而它无从判断
/// 是自己搜错了还是本来就没有。
fn most_recent<T>(items: &[T], limit: usize) -> (&[T], bool) {
    (&items[..items.len().min(limit)], items.len() > limit)
}

#[cfg(test)]
mod tests {
    use super::most_recent;

    #[test]
    fn fewer_than_the_limit_means_there_is_nothing_earlier() {
        assert_eq!(most_recent(&[1, 2], 5), (&[1, 2][..], false));
    }

    /// **恰好 N 条不算「还有更早的」**——这是那个 off-by-one 的落点。
    #[test]
    fn exactly_the_limit_is_not_more() {
        assert_eq!(most_recent(&[1, 2, 3], 3), (&[1, 2, 3][..], false));
    }

    /// 超过 N 条时只带前 N 条，并如实说明还有更早的
    /// （spec: `cli-event-stream` 的「收件箱条目多于 N 条」）。
    #[test]
    fn more_than_the_limit_is_truncated_and_flagged() {
        assert_eq!(most_recent(&[1, 2, 3, 4], 2), (&[1, 2][..], true));
    }

    /// 上限为 0 时一条都不带，但仍要说明有更早的——`--inbox-limit 0` 是合法输入
    /// （「我自己会查，别在基线里搬」），切片越界会让它变成一次 panic。
    #[test]
    fn a_zero_limit_carries_nothing_yet_still_flags() {
        assert_eq!(most_recent(&[1, 2], 0), (&[][..], true));
        assert_eq!(most_recent::<i32>(&[], 0), (&[][..], false));
    }
}
