//! 向用户提问。
//!
//! 与 [`crate::render`] 分开是刻意的：那边是单向输出，这边要读 stdin，因此依赖
//! 「此刻有没有终端」这个环境事实。混进 render 会让这个能力判断散进每一处渲染，
//! 而它恰恰是**必须集中回答一次**的问题——命令行宿主同样跑在服务单元、cron 与
//! agent harness 里，那些场景没有人可问，问了就是挂住。

use std::io::IsTerminal;

use crate::exit::{CliError, CliResult};
use std::sync::atomic::{AtomicBool, Ordering};

/// 用户是否显式禁止了交互（`--no-input`）。
static NO_INPUT: AtomicBool = AtomicBool::new(false);
/// 是否处于结构化输出模式（`--json`）。
static STRUCTURED: AtomicBool = AtomicBool::new(false);

/// 记下本次运行的交互能力。**在分派任何命令之前调用一次。**
///
/// 做成进程级状态而不是逐层传参：这两位是**环境事实**，不是某条命令的参数。
/// 让它们跟着调用链走，等于要求每一个可能提问的函数都多带两个布尔——而漏掉任何一处
/// 的表现是「在不该问的地方问了」，也就是挂住。
pub fn configure(no_input: bool, json: bool) {
    NO_INPUT.store(no_input, Ordering::SeqCst);
    STRUCTURED.store(json, Ordering::SeqCst);
}

/// 此刻能不能向用户提问。
///
/// 四条判据缺一不可：
///
/// - **stdin 是终端**——决定读不读得到回答。
/// - **stderr 是终端**——决定提示看不看得见。只判 stdin 的话，
///   `swarmdrop invite create 2>/dev/null` 会变成一个没有提示的光标，用户不知道程序在等什么。
/// - **未开 `--json`**——那个开关声明的是「调用方是程序」，而程序不会阅读绘制在 stderr 上的
///   选择菜单。即使 stdin 恰好是终端也不问。
/// - **未给 `--no-input`**——TTY 检测有测不准的场景（部分 CI 会分配伪终端），
///   显式开关是脚本与 agent 的逃生口。
pub fn can_ask() -> bool {
    !NO_INPUT.load(Ordering::SeqCst)
        && !STRUCTURED.load(Ordering::SeqCst)
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
}

/// 用户是否**显式**关掉了交互（`--no-input` 或 `--json`），而不是环境碰巧不支持。
///
/// 两者要分开：显式关掉是一个**明确的意图**，命令应当照常运行、只是不问人（例如
/// `invite create --no-input` 生成邀请并守着，期间到达的配对请求一律拒绝）；
/// 而「环境问不了人且用户什么也没说」意味着这条命令注定做不成它该做的事，应当直接
/// 以用法错误退出，而不是默默地把每一个入站请求都拒掉。
pub fn interaction_declined() -> bool {
    NO_INPUT.load(Ordering::SeqCst) || STRUCTURED.load(Ordering::SeqCst)
}

/// 问一个是 / 否，默认否。
///
/// **默认必须是否**：这个提问的用途是守住配对这道信任边界、以及拦住不可逆的批量撤销，
/// 而用户对一个看不懂的提示最可能的反应是直接回车。回车放行等于没问。
///
/// 返回 `None` = **读不到回答**（终端被关掉、管道断开、用户按了 Ctrl-C）。
/// 它与 `Some(false)` 刻意分开：两者都不该放行，但后续动作不同——明确的「否」意味着
/// 「拒了这个、继续等下一个」，而读不到回答意味着「这个人已经走了」，此时还打印一句
/// 「仍在等待」只会与紧随其后的中止自相矛盾。
///
/// 提示走 stderr：stdout 在结构化模式下只能有最终结果。
pub async fn confirm(prompt: String) -> Option<bool> {
    // dialoguer 是同步阻塞的，直接在异步运行时里调会卡住整个 worker——
    // 而本进程的节点、传输与本地通道全都跑在同一个运行时上。
    tokio::task::spawn_blocking(move || {
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .show_default(true)
            .interact_on(&dialoguer::console::Term::stderr())
            .ok()
    })
    .await
    .ok()
    .flatten()
}

/// 让用户从若干项里挑一个，返回选中项的下标。
///
/// 返回 `None` = 读不到回答（同 [`confirm`]）。**没有默认选中项**：这个菜单的用途是
/// 补全一个不可逆操作的目标（撤销哪张邀请、解除哪台设备），预选一项等于在用户直接
/// 回车时替他做了那个决定。
///
/// 菜单绘制在 stderr——它是过程信息，不是命令的结果。
pub async fn select(prompt: String, items: Vec<String>) -> Option<usize> {
    tokio::task::spawn_blocking(move || {
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(&items)
            .interact_on_opt(&dialoguer::console::Term::stderr())
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}

/// 让用户从一组记录里挑一个，挑不了就以用法错误退出。
///
/// 三态里的后两态收在这里（第一态「参数已给出」由调用方在此之前处理完）：
/// 能问就问，问不了就**立刻退出**而不是替用户挑一个。
///
/// **绝不退化成「选第一条」**：这个菜单的用途是补全一个不可逆操作的目标——解除配对、
/// 撤销邀请——替用户做那个决定的后果没有 undo。
///
/// `unavailable` 由调用方给：它要说清「怎么用参数指定」和「去哪看有哪些」，
/// 而那两句话每条命令都不一样。
pub async fn choose_one<'a, T>(
    rows: &'a [T],
    prompt: &str,
    unavailable: &str,
    label: impl Fn(&T) -> String,
) -> CliResult<&'a T> {
    if !can_ask() {
        return Err(CliError::Usage(unavailable.into()));
    }

    let items: Vec<String> = rows.iter().map(label).collect();
    // 读不到回答 ⇒ 用户中止。
    let index = select(prompt.to_owned(), items)
        .await
        .ok_or(CliError::Aborted)?;

    rows.get(index)
        .ok_or_else(|| CliError::Usage("选择超出范围".into()))
}

/// 测试之间串行化对交互状态的改动。
///
/// [`configure`] 写的是进程级状态，而 `cargo test` 默认并行——两个测试同时改它，
/// 断言看到的就是对方的设置。持有这把锁的测试独占那份状态。
///
/// 用 tokio 的锁而不是 `std::sync::Mutex`：持有期要跨 `await`（被串行化的测试体里就有
/// 超时等待），而 std 的 guard 跨 await 会阻塞整个 worker 线程。
#[cfg(test)]
pub async fn interaction_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--no-input` 与 `--json` 各自都足以一票否决交互。
    ///
    /// 这条看守的是两个独立的承诺：前者是脚本的逃生口（TTY 检测会测不准），
    /// 后者是「调用方是程序」的声明。任何一个失效，管道里的命令都会去等一个
    /// 永不到来的回答——表现是永久挂起且日志无异常。
    // async 只为拿那把串行锁（见 `interaction_test_guard`），测试本身不涉及异步。
    #[tokio::test]
    async fn either_flag_disables_interaction() {
        let _guard = interaction_test_guard().await;

        configure(true, false);
        assert!(!can_ask(), "--no-input 必须禁用交互");

        configure(false, true);
        assert!(!can_ask(), "--json 必须禁用交互");

        // 复位，免得影响同进程内的其他测试。
        configure(false, false);
    }
}
