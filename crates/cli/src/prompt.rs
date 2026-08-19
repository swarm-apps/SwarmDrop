//! 向用户提问。
//!
//! 与 [`crate::render`] 分开是刻意的：那边是单向输出，这边要读 stdin，因此依赖
//! 「此刻有没有终端」这个环境事实。混进 render 会让这个能力判断散进每一处渲染，
//! 而它恰恰是**必须集中回答一次**的问题——命令行宿主同样跑在服务单元、cron 与
//! agent harness 里，那些场景没有人可问，问了就是挂住。

use std::io::IsTerminal;

/// 此刻能不能向用户提问。
///
/// **stdin 与 stderr 都要是终端**：前者决定读不读得到回答，后者决定提示看不看得见。
/// 只判 stdin 的话，`swarmdrop pair 2>/dev/null` 会变成一个没有提示的光标——
/// 用户不知道程序在等什么。
pub fn can_ask() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// 问一个是 / 否，默认否。
///
/// **默认必须是否**：这个提问唯一的用途是守住配对这道信任边界，而用户对一个看不懂的
/// 提示最可能的反应是直接回车。回车放行等于没问。
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
