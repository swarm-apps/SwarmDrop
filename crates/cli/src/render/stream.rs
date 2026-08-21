//! `swarmdrop watch` 的事件流输出。
//!
//! ⚠️ **不要与 [`super::panel`] 混起来**：那是 `transfer watch` 的面板（重绘全量快照、
//! 给人看、走 stderr）。本模块是增量事件流。
//!
//! ## 两种模式都走 stdout
//!
//! 这与本层「进度与诊断走 stderr」的默认相反，判据是**这条流就是命令的结果**——
//! `swarmdrop watch` 唯一产出的东西就是它。宿主 spawn 它、读它的 stdout，
//! 那是这条命令存在的全部理由（spec: `cli-event-stream`）。
//!
//! ## 为什么吃 [`Value`] 而不是订阅面的 typed 事件
//!
//! 事件多数来自**通道对面的常驻节点**，而它完全可能比本进程新（升级 CLI 不会重启常驻
//! 节点）。本进程在这条流上是转发者不是解释者：不认识的事件类型原样流过才是对的，
//! 反序列化成 typed 值会在那一刻失败，把一条本可正常流过的记录变成订阅中断。
//!
//! 人类可读那一半因此是**尽力而为**的：认得的类型给一句人话，认不得的原样打 JSON。

use serde_json::Value;

use super::transfer::{direction_glyph, phase_label};
use super::{
    array_or_empty as array, blank_as_placeholder, bytes_or_dash, int_or_zero, short, text_or,
};

/// 会话标识在人类可读输出里的截断长度。
const ID_CHARS: usize = 8;

/// 写出一条事件。返回 `false` = **消费方已经把这条流关掉了**，没有人在听了。
///
/// **只负责把一条已经盖好版本与序号的事件变成一行**。盖章不在这里——那是线格式契约的
/// 一部分，归 [`crate::runtime::watch::client`]（本层不含业务判断，见 [`super`]）。
///
/// 自由函数而不是一个持有 `json` 的类型：这条流上没有任何跨行的状态需要维护
/// （序号已经搬走了），一个只装着一个 `bool` 的结构体只是多一个要理解的名字。
///
/// ⚠️ **不用 `println!`。** Rust 启动时把 `SIGPIPE` 设成忽略，于是往一条已关闭的管道写
/// 会让 `write` 返回 `EPIPE`，而 `println!` 对此的反应是 **panic**
/// （`failed printing to stdout`，退出码 101）。这条命令长驻数天、唯一的产出就是往 stdout
/// 写——宿主结束订阅时先关读端是它的常规动作，而 101 会被读成「它崩了」并触发自动重启。
///
/// 也**不能靠把 `SIGPIPE` 恢复成默认处置**来一劳永逸：那正是 Rust 忽略它的原因——
/// 本进程跑着 P2P 栈，Linux 上对一条已关闭的 TCP 连接 `write` 同样会抬 `SIGPIPE`，
/// 恢复默认等于让任何一次对端断连都可能直接杀掉节点。
pub fn write(event: &Value, json: bool) -> bool {
    let line = if json {
        // **一行一条，不 pretty**：NDJSON 的整个前提是「每行自成一条完整事件」，
        // 缩进过的 JSON 跨行，消费方按行读会当场解析失败。
        match serde_json::to_string(event) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("序列化订阅事件失败: {err}");
                return true; // 少一行，但流还在
            }
        }
    } else {
        human(event)
    };

    use std::io::Write;
    match writeln!(std::io::stdout().lock(), "{line}") {
        Ok(()) => true,
        // 读端走了。**这不是错误**，不必往 stderr 说什么——它是这条流的正常终点。
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => false,
        // 其余写失败（磁盘满、终端没了）同样意味着这条流没法继续，但值得留一句。
        Err(err) => {
            eprintln!("写出订阅事件失败: {err}");
            false
        }
    }
}

/// 一条事件的人话形态。
///
/// 认不得的类型**原样打 JSON 而不是跳过**：跳过会让人以为什么都没发生，
/// 而这条命令的用途正是「告诉我发生了什么」。
fn human(event: &Value) -> String {
    let kind = event.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "baseline" => baseline_line(event),
        "inboxAdded" => format!(
            "收件箱  来自 {} 的 {} 项（{}）",
            blank_as_placeholder(&text_or(event, "sourceName", "—")),
            int_or_zero(event, "itemCount"),
            bytes_or_dash(event.get("totalSize")),
        ),
        "inboxArchived" => {
            let verb = if event.get("archived").and_then(Value::as_bool) == Some(true) {
                "归档"
            } else {
                "取消归档"
            };
            format!(
                "收件箱  {verb} {}",
                short(&text_or(event, "itemId", "—"), ID_CHARS)
            )
        }
        "inboxRemoved" => format!(
            "收件箱  删除 {}",
            short(&text_or(event, "itemId", "—"), ID_CHARS)
        ),
        "transferChanged" => format!(
            "传输    {} {}  {}  {} / {}",
            direction_glyph(event.get("direction").and_then(Value::as_str)),
            blank_as_placeholder(&text_or(event, "peerName", "—")),
            phase_label(event),
            bytes_or_dash(event.get("transferredBytes")),
            bytes_or_dash(event.get("totalBytes")),
        ),
        "transferProgress" => format!(
            "进度    {} {}  {} / {}  文件 {}/{}",
            direction_glyph(event.get("direction").and_then(Value::as_str)),
            short(&text_or(event, "sessionId", "—"), ID_CHARS),
            bytes_or_dash(event.get("transferredBytes")),
            bytes_or_dash(event.get("totalBytes")),
            int_or_zero(event, "completedFiles"),
            int_or_zero(event, "totalFiles"),
        ),
        "devicesChanged" => {
            let devices = array(event, "devices");
            format!(
                "设备    已配对 {} 台，在线 {} 台",
                devices.len(),
                online_count(devices)
            )
        }
        "nodeUnavailable" => "节点    常驻节点已停止，继续等待".into(),
        "truncated" => format!(
            "⚠ 截断  跳过了 {} 条事件（本机读得太慢）",
            int_or_zero(event, "dropped")
        ),
        _ => format!("事件    {event}"),
    }
}

fn baseline_line(event: &Value) -> String {
    let devices = array(event, "devices");
    let more = if event.get("inboxHasMore").and_then(Value::as_bool) == Some(true) {
        "+"
    } else {
        ""
    };
    format!(
        "基线    收件箱 {}{} 条 · 已配对 {} 台（在线 {}）· 未完成传输 {} 条 · 节点{}",
        array(event, "inbox").len(),
        more,
        devices.len(),
        online_count(devices),
        array(event, "transfers").len(),
        if event.get("nodeRunning").and_then(Value::as_bool) == Some(true) {
            "运行中"
        } else {
            "未运行"
        },
    )
}

/// 在线设备数。
///
/// **`online` 缺失或为 `null` 不算在线**：那是「未知」（本机节点没跑，没做过探测），
/// 把未知算成在线是一个凭空的断言。
fn online_count(devices: &[Value]) -> usize {
    devices
        .iter()
        .filter(|device| device.get("online").and_then(Value::as_bool) == Some(true))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// **写失败要说得出来，不能 panic。**
    ///
    /// 这条流长驻数天，唯一的产出就是往 stdout 写；宿主结束订阅时先关读端是它的常规
    /// 动作。用 `println!` 的话那一刻是一次 panic（退出码 101），宿主读成「它崩了」
    /// 并自动重启。这里断言的是**返回值承载了这件事**——真正的 EPIPE 造不出来
    /// （测试进程的 stdout 是被 harness 捕获的），能钉住的是「有一个可判断的返回值」。
    #[test]
    fn a_successful_write_reports_that_someone_is_still_listening() {
        assert!(write(&json!({ "kind": "nodeUnavailable" }), true));
    }

    /// NDJSON 的每一行必须**自成一条完整事件**，不能跨行。
    ///
    /// 缩进过的 JSON 看起来更友好，但消费方是按行读的——一次 pretty 打印会让它的
    /// 每一次解析都失败。
    #[test]
    fn structured_output_is_one_line_per_event() {
        let text = serde_json::to_string(&json!({
            "v": 1,
            "seq": 0,
            "kind": "baseline",
            "inbox": [json!({ "itemId": "a" })],
        }))
        .expect("序列化");
        assert!(!text.contains('\n'), "NDJSON 不得跨行: {text}");
    }

    /// 认不得的事件类型**原样打出来**，不静默跳过。
    #[test]
    fn an_unknown_kind_is_printed_verbatim() {
        let line = human(&json!({ "kind": "somethingNewer", "detail": 1 }));
        assert!(line.contains("somethingNewer"), "{line}");
    }

    /// 未知的在线状态不得算成在线。
    #[test]
    fn unknown_online_state_is_not_counted_as_online() {
        let devices = [
            json!({ "online": null }),
            json!({}),
            json!({ "online": true }),
        ];
        assert_eq!(online_count(&devices), 1);
    }

    /// 基线那一行要把「还有更早的条目」说出来——否则用户以为收件箱就这么多。
    #[test]
    fn the_baseline_line_says_there_is_more() {
        let line = baseline_line(&json!({
            "inbox": [json!({}), json!({})],
            "inboxHasMore": true,
            "devices": [],
            "transfers": [],
            "nodeRunning": true,
        }));
        assert!(line.contains("2+"), "{line}");
        assert!(line.contains("运行中"), "{line}");
    }

    /// 缺字段一律降级成占位符，**绝不 panic**：这条流长驻数天，一次 panic 等于订阅消失。
    #[test]
    fn a_malformed_event_degrades_instead_of_panicking() {
        for kind in [
            "baseline",
            "inboxAdded",
            "inboxArchived",
            "inboxRemoved",
            "transferChanged",
            "transferProgress",
            "devicesChanged",
            "truncated",
        ] {
            let line = human(&json!({ "kind": kind }));
            assert!(!line.is_empty(), "{kind} 渲染成了空行");
        }
    }
}
