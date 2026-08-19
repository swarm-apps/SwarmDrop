//! 节点状态与生命周期动作的两套渲染。
//!
//! 输入是**已经序列化好的 JSON**而非核心的 DTO：状态可能来自本进程的节点，也可能来自
//! 通道对面的常驻节点，后者只能是 JSON。让渲染层统一吃 JSON，就不必为两条来源各写一份。

use serde_json::Value;

use super::text_or;

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
