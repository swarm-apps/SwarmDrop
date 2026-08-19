//! 配对相关渲染。

use swarmdrop_core::device::ConnectionType;

use crate::runtime::pairing::{PairOutcome, PairingRequest};

/// 输出邀请：二维码（可关）+ 链接。
///
/// 链接**总是**输出：手机扫码是主路径，但把码复制到另一台电脑同样常见，而 base32 的
/// 邀请串手输不现实。
pub fn render_invite(invite: &str, json: bool, no_qr: bool) {
    if json {
        let payload = serde_json::json!({ "invite": invite });
        println!("{payload}");
        return;
    }

    if !no_qr {
        match swarmdrop_invite::invite_qr_matrix(invite, crate::render::qr::FACE_PX) {
            Ok(matrix) => println!("{}\n", crate::render::qr::render(matrix.as_slice())),
            // 码画不出来不该让整条命令失败——链接仍然可用。
            Err(err) => eprintln!("二维码生成失败（链接仍可用）: {err}"),
        }
    }

    println!("{invite}");
}

/// 等待对方扫码。**写 stderr**：它是过程信息，不是命令结果。
///
/// 三件事必须说到，否则用户会在错误的地方等：这张码活多久、待会儿会不会问他、
/// 以及（临时节点时）命令一退出码就废了。
pub fn render_waiting(temporary_node: bool, auto_accept: bool, json: bool) {
    if json {
        return;
    }

    eprintln!();
    eprintln!("等待对方扫码…（Ctrl-C 取消）");
    if auto_accept {
        eprintln!("⚠️ 已开启自动接受：第一台出示这张邀请的设备会直接配上，不会再问你。");
    } else {
        eprintln!("有设备请求配对时会在这里列出它的信息，由你确认后才会接受。");
    }

    if temporary_node {
        eprintln!("注意：这张码在本命令退出后即失效——邀请的可拨地址就是当前这个进程的节点。");
    }
}

/// 一次入站配对请求：把用户判断所需的信息摆全。
///
/// **完整节点标识单独占一行，不截断**：设备名是对端自己报的、可以随便填成和你手上那台
/// 一模一样，而节点标识是公钥的哈希，与传输层握手校验的是同一个身份。请对方念一遍它的
/// 开头几位，是这里唯一能挡住「被人抢先扫码」的手段。
pub fn render_pairing_request(request: &PairingRequest, json: bool) {
    if json {
        let payload = serde_json::json!({
            "event": "pairingRequest",
            "pendingId": request.pending_id,
            "peerId": request.peer_id,
            "device": request.device,
            "os": request.os,
            "arch": request.arch,
            "connection": request.connection,
        });
        println!("{payload}");
        return;
    }

    eprintln!();
    eprintln!("┌─ 收到配对请求 ─────────────────────────────");
    // 与 `render_paired` 同一个兜底：`OsInfo::display_name` 的文档把占位符留给了视图层，
    // 而对端完全可以报一个空名字——那时打印一个空行比打印占位符更难判断。
    eprintln!("│ 设备      {}", blank_as_placeholder(&request.device));
    eprintln!("│ 系统      {} · {}", request.os, request.arch);
    eprintln!("│ 链路      {}", describe_connection(request.connection));
    eprintln!("│ 节点标识  {}", request.peer_id);
    eprintln!("└────────────────────────────────────────────");
    eprintln!("请与对方核对节点标识后再决定——设备名可以伪造，它不能。");
}

/// 设备名为空时的占位符。
///
/// `OsInfo::display_name` 的文档把占位符留给视图层决定——对端可以报一个空名字，
/// 而打印一个空白比打印占位符更难判断出了什么事。
fn blank_as_placeholder(name: &str) -> &str {
    if name.trim().is_empty() {
        "（未命名设备）"
    } else {
        name
    }
}

/// 链路的说法。
///
/// 穷尽 match 而非带 `_` 的兜底：新增链路类型时这里会编译失败，而不是静默显示成
/// 一个含糊的说法——这一行是用户判断「这是不是我身边那台」的依据之一。
fn describe_connection(connection: Option<ConnectionType>) -> &'static str {
    match connection {
        Some(ConnectionType::Lan) => "局域网直连",
        Some(ConnectionType::Direct) => "直连（公网地址或 VPN 隧道）",
        Some(ConnectionType::Dcutr) => "打洞直连（信令经中继，数据不经）",
        Some(ConnectionType::Relay) => "经中继转发 —— 对方不在本地网络",
        None => "未知（链路刚建立）",
    }
}

/// 拒绝了一次配对请求，但仍在等。
///
/// **要说清楚「码还能用」**：用户拒掉一个陌生请求后最担心的就是自己那张码废了。
pub fn render_declined(request: &PairingRequest, json: bool) {
    if json {
        let payload = serde_json::json!({
            "event": "pairingDeclined",
            "pendingId": request.pending_id,
            "peerId": request.peer_id,
        });
        println!("{payload}");
        return;
    }
    eprintln!("已拒绝。这张邀请没有被消耗，仍在等待——继续扫码即可。");
    eprintln!();
}

/// 待确认的请求在确认期间失效了。
pub fn render_request_expired(json: bool) {
    if json {
        println!(r#"{{"event":"pairingRequestExpired"}}"#);
        return;
    }
    eprintln!("这条配对请求已失效（对方断开或等待超时），仍在等待下一次——继续扫码即可。");
    eprintln!();
}

/// 已发出配对请求，正在等对方点同意。
///
/// **必须给出这句**：对端要有人去点确认，而这一侧此前是完全静默的，
/// 看起来与卡死没有区别（对端不点的话要等满 3 分钟）。
pub fn render_awaiting_confirmation(inviter: &str, json: bool) {
    if !json {
        eprintln!("已向 {inviter} 发出配对请求，等待对方确认…（最长 3 分钟）");
    }
}

/// 配对成功。
pub fn render_paired(outcome: &PairOutcome, json: bool) {
    let device = blank_as_placeholder(outcome.device.as_deref().unwrap_or_default());
    if json {
        let payload = serde_json::json!({
            "event": "paired",
            "device": device,
            "persisted": outcome.persisted,
        });
        println!("{payload}");
        return;
    }

    println!("配对成功：{device}");
    if !outcome.persisted {
        // 如实说后果，而不是说「配对失败」——配对是成的，丢的是这台机器上的记录。
        eprintln!("警告：这台设备没能写入本机配对表，重启后会从列表里消失（对方仍记着）。");
    }
}
