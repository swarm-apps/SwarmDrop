//! 节点状态与生命周期动作的两套渲染。
//!
//! 输入是**已经序列化好的 JSON**而非核心的 DTO：状态可能来自本进程的节点，也可能来自
//! 通道对面的常驻节点，后者只能是 JSON。让渲染层统一吃 JSON，就不必为两条来源各写一份。

use serde_json::Value;

use super::text_or;
use crate::runtime::ipc;

/// 渲染状态快照。结果走 stdout（人类可读与结构化都是「命令的结果」）。
pub fn render(status: &Value, json: bool) {
    if json {
        super::emit_json(status, "状态");
        return;
    }

    println!("状态      {}", text_or(status, "status", "未知"));
    if let Some(peer) = status.get("peerId").and_then(Value::as_str) {
        println!("节点标识  {peer}");
    }
    println!("NAT       {}", text_or(status, "natStatus", "未知"));
    if let Some(addr) = status.get("publicAddr").and_then(Value::as_str) {
        println!("公网地址  {addr}");
    }

    render_infra(status);

    let addrs = status
        .get("listenAddrs")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if addrs.is_empty() {
        println!("监听地址  （无）");
    } else {
        println!("监听地址");
        for addr in addrs {
            if let Some(addr) = addr.as_str() {
                println!("          {addr}");
            }
        }
    }

    // 最后：它是旁白而不是状态的一部分，夹在监听地址前面会被那份清单埋掉。
    render_version_skew(status);
}

/// 常驻节点与本命令不是同一个版本时的一行旁白。
///
/// **只在不一致时出现**：相等是常态，说出来只是噪音。走 stderr —— 这是关于「你的机器
/// 处于什么状态」的旁白，不是 `status` 这条命令的结果。
///
/// 两种不一致，判据不同：
///
/// - **报告了版本且不等于本命令的**：直说两个数。
/// - **在跑却没报告版本**：那个节点比引入版本戳的那一版旧（见
///   [`ipc::DAEMON_VERSION_KEY`]）。**「在跑」这个前提不能省**——没有节点时
///   `cmd::status` 用一份 `NetworkStatus::default()` 顶上，那份同样没有这个字段，
///   而对着一台根本没起节点的机器说「你的节点太旧」是纯粹的误导。
///
/// `--json` 不走这里：结构化消费者能直接读 `daemonVersion` 自己判断，用不着一行中文。
fn render_version_skew(status: &Value) {
    let Some(skew) = version_skew(status) else {
        return;
    };
    match skew {
        Skew::Reported(daemon) => {
            eprintln!("常驻节点是 {daemon}，这条命令是 {}。", ipc::DAEMON_VERSION);
        }
        Skew::Silent => eprintln!("常驻节点比这条命令旧（它还不会报告自己的版本）。"),
    }
    eprintln!("新加的命令它可能认不得；先 swarmdrop stop 再 swarmdrop start 让它用上当前版本。");
}

/// [`render_version_skew`] 的判据，与打印分开是为了能测——那三条分支里有两条只在
/// 特定组合下成立，靠肉眼读是这类旁白最容易说反的地方。
#[derive(Debug, PartialEq, Eq)]
enum Skew {
    /// 常驻节点报告了版本，且与本命令不同。
    Reported(String),
    /// 节点在跑却没报告版本 —— 它比引入版本戳的那一版旧。
    Silent,
}

fn version_skew(status: &Value) -> Option<Skew> {
    match status.get(ipc::DAEMON_VERSION_KEY).and_then(Value::as_str) {
        Some(daemon) if daemon != ipc::DAEMON_VERSION => Some(Skew::Reported(daemon.to_owned())),
        Some(_) => None,
        // 「在跑」这个前提不能省，理由见 [`render_version_skew`]。
        None if status.get("status").and_then(Value::as_str) == Some("running") => {
            Some(Skew::Silent)
        }
        None => None,
    }
}

/// 引导 / 中继一行。
///
/// **节点没跑时什么都不说**：那时 `infraLinks` 恒空，而「没有引导节点」与「节点没起、
/// 所以还没有任何关系」是两回事，把后者渲染成前者会让用户去改一份根本没问题的配置。
///
/// 节点在跑而清单是空的，则**必须说出后果**：清空到零条是允许的（用户可能只在局域网内
/// 用），所以写入路径不拦它——那就只剩这里有机会告诉用户他现在跨网不可达。
fn render_infra(status: &Value) {
    if status.get("status").and_then(Value::as_str) != Some("running") {
        return;
    }

    let links = super::array_or_empty(status, "infraLinks");
    if links.is_empty() {
        println!("引导节点  （无）");
        eprintln!("没有引导 / 中继节点，本机只能在局域网内发现设备。");
        eprintln!("用 swarmdrop bootstrap add <multiaddr> 加一条。");
        return;
    }

    let connected = links
        .iter()
        .filter(|link| super::flag(link, "connected"))
        .count();
    let relay_ready = links
        .iter()
        .filter(|link| {
            link.get("relay")
                .and_then(|relay| relay.get("kind"))
                .and_then(Value::as_str)
                == Some("active")
        })
        .count();
    println!(
        "引导节点  {} 条（{connected} 已连接 · {relay_ready} 条中继就绪）",
        links.len()
    );
}

/// 前台启动就绪。
pub fn render_started(node_id: &swarmdrop_net::NodeId, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "event": "started", "nodeId": node_id.to_string() })
        );
    } else {
        println!("节点已启动  {node_id}");
        println!("按 Ctrl-C 停止，或在另一个终端执行 swarmdrop stop");
    }
}

/// 后台启动的结果。`ready=false` 表示等待超时——不是失败，只是还没就绪。
pub fn render_detached(ready: bool, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "event": "detached", "ready": ready })
        );
    } else if ready {
        println!("节点已在后台启动");
    } else {
        println!("已在后台拉起节点，但等待就绪超时；可用 swarmdrop status 查看");
    }
}

/// 停止的结果。`stopped=false` 表示本来就没有节点在运行。
pub fn render_stopped(stopped: bool, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "event": "stopped", "wasRunning": stopped })
        );
    } else if stopped {
        println!("节点已停止");
    } else {
        println!("当前没有节点在运行");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_is_not_worth_saying() {
        let value = serde_json::json!({
            "status": "running",
            ipc::DAEMON_VERSION_KEY: ipc::DAEMON_VERSION,
        });
        assert_eq!(version_skew(&value), None);
    }

    #[test]
    fn reports_both_versions_when_they_differ() {
        let value = serde_json::json!({
            "status": "running",
            ipc::DAEMON_VERSION_KEY: "0.0.1-ancient",
        });
        assert_eq!(
            version_skew(&value),
            Some(Skew::Reported("0.0.1-ancient".into()))
        );
    }

    /// 在跑却不报版本 = 比引入版本戳的那一版旧。
    #[test]
    fn a_running_node_that_reports_nothing_is_old() {
        let value = serde_json::json!({ "status": "running" });
        assert_eq!(version_skew(&value), Some(Skew::Silent));
    }

    /// **没有节点时必须闭嘴。** `cmd::status` 在通道连不上时用一份
    /// `NetworkStatus::default()` 顶上，它同样没有版本字段——少了「在跑」这个前提，
    /// 每一台没起节点的机器都会被告知「你的节点太旧」。
    #[test]
    fn no_node_is_not_a_version_problem() {
        let value = serde_json::to_value(swarmdrop_core::network::NetworkStatus::default())
            .expect("序列化默认状态");
        assert_eq!(
            value.get(ipc::DAEMON_VERSION_KEY),
            None,
            "默认状态不该带版本戳"
        );
        assert_eq!(version_skew(&value), None);
    }
}
