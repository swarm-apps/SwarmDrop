//! 节点状态与生命周期动作的两套渲染。
//!
//! 输入是**已经序列化好的 JSON**而非核心的 DTO：状态可能来自本进程的节点，也可能来自
//! 通道对面的常驻节点，后者只能是 JSON。让渲染层统一吃 JSON，就不必为两条来源各写一份。

use serde_json::Value;

/// 渲染状态快照。结果走 stdout（人类可读与结构化都是「命令的结果」）。
pub fn render(status: &Value, json: bool) {
    if json {
        // 结果是唯一写进 stdout 的东西，序列化失败也不能退化成半截输出——
        // 宁可什么都不写，让调用方按退出码判断。
        match serde_json::to_string_pretty(status) {
            Ok(text) => println!("{text}"),
            Err(err) => eprintln!("序列化状态失败: {err}"),
        }
        return;
    }

    println!("状态      {}", text_of(status, "status"));
    if let Some(peer) = status.get("peerId").and_then(Value::as_str) {
        println!("节点标识  {peer}");
    }
    println!("NAT       {}", text_of(status, "natStatus"));
    if let Some(addr) = status.get("publicAddr").and_then(Value::as_str) {
        println!("公网地址  {addr}");
    }

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
}

/// 取一个字段的可读文本。枚举在 JSON 里可能是字符串，也可能是带标签的对象。
fn text_of(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "未知".into(),
    }
}

/// 前台启动就绪。
pub fn render_started(node_id: &swarmdrop_net::NodeId, json: bool) {
    if json {
        println!(r#"{{"event":"started","nodeId":"{node_id}"}}"#);
    } else {
        println!("节点已启动  {node_id}");
        println!("按 Ctrl-C 停止，或在另一个终端执行 swarmdrop stop");
    }
}

/// 后台启动的结果。`ready=false` 表示等待超时——不是失败，只是还没就绪。
pub fn render_detached(ready: bool, json: bool) {
    if json {
        println!(r#"{{"event":"detached","ready":{ready}}}"#);
    } else if ready {
        println!("节点已在后台启动");
    } else {
        println!("已在后台拉起节点，但等待就绪超时；可用 swarmdrop status 查看");
    }
}

/// 停止的结果。`stopped=false` 表示本来就没有节点在运行。
pub fn render_stopped(stopped: bool, json: bool) {
    if json {
        println!(r#"{{"event":"stopped","wasRunning":{stopped}}}"#);
    } else if stopped {
        println!("节点已停止");
    } else {
        println!("当前没有节点在运行");
    }
}
