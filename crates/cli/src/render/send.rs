//! 发送结果渲染。

use crate::runtime::transfer::SendOutcome;

pub fn render(outcome: &SendOutcome, json: bool) {
    if json {
        let payload = serde_json::json!({
            "sessionId": outcome.session_id.to_string(),
            "fileCount": outcome.file_count,
            "totalBytes": outcome.total_bytes,
        });
        println!("{payload}");
    } else {
        println!(
            "已发送 {} 个文件（{}）",
            outcome.file_count,
            human_bytes(outcome.total_bytes)
        );
    }
}

/// 结果来自通道对面的常驻节点时，它已经是 JSON。
pub fn render_from_json(payload: &serde_json::Value, json: bool) {
    if json {
        println!("{payload}");
        return;
    }
    let count = payload
        .get("fileCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let bytes = payload
        .get("totalBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    println!("已发送 {count} 个文件（{}）", human_bytes(bytes));
}

/// 人类可读的字节数。
///
/// 用 1024 进制并标注 KiB/MiB：文件大小在磁盘与传输语境下都是二进制单位，
/// 用 1000 进制会与用户在文件管理器里看到的数对不上。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 传输进度条。
///
/// **画在 stderr**：结构化模式下 stdout 只能有最终结果，而人类可读模式下把进度混进
/// 结果也不利于管道使用。
///
/// 走 `indicatif` 而不是自己 `\r` 刷新，换来的是三件自写版本没有的事：
///
/// - **非终端时自动静默**。自写版本在 `swarmdrop send … | tee log` 或 CI 里照样输出
///   回车控制符，日志文件里于是变成一行几百个 `\r` 拼起来的乱码。
/// - **速率与剩余时间**。传大文件时这两个数才是用户真正在等的答案，
///   而它们要维护一个时间窗口，不是「再加一行 format!」能顺手做对的。
/// - **重绘时清行**。stderr 上还有 tracing 的日志，自写版本被日志插一行后会留下
///   半截残影，直到下一次刷新才被覆盖。
pub struct Progress(indicatif::ProgressBar);

/// 进度条模板。
///
/// **必须是常量**：模板写错时 [`Progress::new`] 回退到默认样式，进度条照常出现、
/// 只是速率与剩余时间不见了，没有任何报错。看守它的测试如果自己抄一份字面量，
/// 改坏这里、忘了改那里，测试仍然绿——那条护栏就等于不存在。
/// **不用 indicatif 的 `{binary_bytes}`**：它给两位小数（`1.00 MiB`），而同一条 `send`
/// 结束时打印的结果行走 [`human_bytes`]（一位小数，`1.0 MiB`）——同一个数在同一屏里
/// 两种写法。改用自定义的 key 把两处都接到 `human_bytes` 上。
const TEMPLATE: &str = "传输中 {bar:24} {percent:>3}%  {done}/{total}  {rate}  剩余 {eta}";

impl Progress {
    /// `enabled` 为假（结构化输出模式）时返回一个不绘制任何东西的实例。
    ///
    /// 不用 `Option<Progress>` 是刻意的：调用点会因此散落 `if let Some(..)`，
    /// 而「什么时候该画」必须只回答一次。stderr 不是终端时由 indicatif 自己隐藏。
    pub fn new(enabled: bool) -> Self {
        let bar = if enabled {
            indicatif::ProgressBar::no_length()
        } else {
            indicatif::ProgressBar::hidden()
        };
        bar.set_style(
            indicatif::ProgressStyle::with_template(TEMPLATE)
                // 写错只会在运行时退化成默认样式——由 `progress_template_is_valid` 钉住。
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
                .with_key(
                    "done",
                    |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        let _ = write!(w, "{}", human_bytes(state.pos()));
                    },
                )
                .with_key(
                    "total",
                    |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        let _ = write!(w, "{}", human_bytes(state.len().unwrap_or(0)));
                    },
                )
                .with_key(
                    "rate",
                    |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                        let _ = write!(w, "{}/s", human_bytes(state.per_sec() as u64));
                    },
                ),
        );
        Self(bar)
    }

    /// 收到一条进度事件。
    ///
    /// 总量每次都重设：它由第一条事件才带过来，而续传场景下同一会话的总量可能变化。
    pub fn update(&self, transferred: u64, total: u64) {
        self.0.set_length(total);
        self.0.set_position(transferred);
    }
}

/// 离开作用域即收掉进度条。
///
/// **不做成 `finish()` 方法在每个返回点各调一次**：等待终态的那个循环有四条出口
/// （完成、失败、拒绝、通道断开），只有第一条会自然想起要收尾——旧版本就漏了另外三条，
/// 于是「传输失败: …」直接印在那条没有换行的进度行后面。新增一种终态时同样会漏。
///
/// `finish_and_clear` 而非 `finish`：最终结果紧接着打印在 stdout 上，
/// 留一条走完的进度条在旁边只是把同一个数字再说一遍。
impl Drop for Progress {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进度条模板必须可解析。
    ///
    /// [`Progress::new`] 在模板写错时回退到默认样式——那是**静默**降级：进度条照常
    /// 出现，只是速率与剩余时间不见了，而没有人会因此收到报错。
    #[test]
    fn progress_template_is_valid() {
        assert!(
            indicatif::ProgressStyle::with_template(TEMPLATE).is_ok(),
            "模板无法解析，Progress::new 会静默退回默认样式"
        );
    }

    /// 结构化输出模式下一个字节都不能画——stdout 的解析方会被进度条冲掉。
    #[test]
    fn structured_mode_draws_nothing() {
        assert!(Progress::new(false).0.is_hidden());
    }

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    }
}
