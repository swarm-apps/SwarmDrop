//! 设备清单与解除配对的渲染。

use crate::runtime::devices::{DeviceRow, ForgetOutcome};

use super::blank_as_placeholder;

/// 在线状态的标记。
///
/// **三态而非两态**：`None` 是「未知」（本机没启动节点，从未探测过），把它显示成离线
/// 会让用户去排查网络，而真实原因是节点没开。
fn presence(row: &DeviceRow) -> &'static str {
    match row.online {
        Some(true) => "●",
        Some(false) => "○",
        None => "·",
    }
}

pub fn render_list(rows: &[DeviceRow], json: bool) {
    if json {
        super::emit_json(rows, "设备列表");
        return;
    }

    if rows.is_empty() {
        println!("尚无已配对设备。执行 swarmdrop invite create 生成邀请。");
        return;
    }

    for row in rows {
        println!("{} {}", presence(row), blank_as_placeholder(&row.name));
        println!("   {}", row.peer_id);
    }

    // 只在确实有未知项时才解释，别在正常情况下多说一句。
    if rows.iter().any(|row| row.online.is_none()) {
        eprintln!();
        eprintln!("· = 在线状态未知（节点未运行，本机没有探测过）。执行 swarmdrop start 后再看。");
    }
}

/// 选择菜单里的一行。信息要够用户分辨出是哪台。
pub fn menu_line(row: &DeviceRow) -> String {
    let short = row.peer_id.chars().take(12).collect::<String>();
    format!(
        "{}  ({} · {}…)",
        blank_as_placeholder(&row.name),
        row.os,
        short
    )
}

pub fn render_forgotten(row: &DeviceRow, outcome: &ForgetOutcome, json: bool) {
    if json {
        let payload = serde_json::json!({
            "event": "deviceForgotten",
            "peerId": outcome.peer_id,
            "name": row.name,
            "remaining": outcome.remaining,
        });
        println!("{payload}");
        return;
    }

    println!("已解除与「{}」的配对。", blank_as_placeholder(&row.name));
    // **必须说清楚这是单方面的**：用户会以为对方也不再认识这台机器了。
    eprintln!("注意：这只移除了本机的记录，对方可能仍记着你——需要的话请对方也解除一次。");
    eprintln!("本机还记着 {} 台设备。", outcome.remaining);
}
