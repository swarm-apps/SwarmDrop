//! 配对邀请相关渲染。

use swarmdrop_core::device::ConnectionType;

use super::blank_as_placeholder;
use crate::runtime::invites::{InviteRow, RevokeOutcome};
use crate::runtime::pairing::{PairOutcome, PairingRequest};

/// 输出刚生成的邀请：链接 + **标识**。
///
/// **终端里不画二维码**（2026-08-20 起，此前画半块字符码并附 `--no-qr` 开关）。
/// 它的尺寸没有可调空间：邀请体积的大头是签名 64B + 公钥 32B + capability 16B 与 42 字符的
/// 链接前缀，地址只占零头——实测把地址裁到 `fit` 的下界（只剩一条）仍是 69 模块，也就是
/// **69 列 × 35 行**，标准 80×24 终端里一屏半，而 `render_waiting` 之后还要继续往下写。
/// 半块（1 列 = 1 模块、1 行 = 2 模块）又已经是长宽比正确的最密形态：换成 2×2 象限块能把
/// 宽度减半，但终端字符本身高约两倍于宽，码会被压成 2:1 的竖长条，扫码器直接读不出。
///
/// **扫码这条路径没有断，只是换了承载**：canonical 链接指向落地页 `/p/`，那一页按需渲染
/// 二维码（它是浏览器，没有终端的宽高比与行数约束）。所以这里必须给出那句引导——
/// 少了它，用户看到的就是「二维码没了」。
///
/// 链接走 stdout：它是命令结果本身，`> invite.txt` 要拿到的就是它。
///
/// **标识必须给出**（`id`）：邀请清单里没有邀请串本身（capability 明文不落盘），能区分
/// 多张的信息只有标识与时刻——一分钟内发两张时仅凭时刻分不出哪张发给了谁。它是
/// 「刚发错人、立刻撤回」这条主场景可用的前提。取不到时省略，不让整条命令失败。
pub fn render_created(invite: &str, id: Option<&str>, json: bool) {
    if json {
        let payload = serde_json::json!({ "invite": invite, "id": id });
        println!("{payload}");
        return;
    }

    println!("{invite}");
    // 紧跟链接，且走 stderr：它是「拿这条链接怎么办」的引导，不是命令结果的一部分。
    eprintln!("手机扫码：在浏览器打开上面的链接，页面会显示二维码。");
    if let Some(id) = id {
        // 走 stdout：它是命令结果的一部分（撤销时要用），不是过程信息。
        println!();
        println!("邀请标识  {id}");
        eprintln!("发错了可以撤回：swarmdrop invite revoke {}", short(id));
    }
}

/// 邀请标识的短形式——撤销接受唯一前缀，提示里给短的更可能被真的敲进去。
///
/// 8 位是「够短到愿意敲」与「够长到唯一」之间的取值：`MIN_PREFIX` 只要求 4 位，
/// 而个人用途的未过期邀请通常是个位数条。
fn short(id: &str) -> String {
    super::short(id, 8)
}

/// 已发出邀请的清单。
pub fn render_list(rows: &[InviteRow], json: bool) {
    if json {
        super::emit_json(rows, "邀请清单");
        return;
    }

    if rows.is_empty() {
        println!("没有尚未过期的邀请。");
        return;
    }

    // 循环外读一次——每行各读一次的话，同一屏里的「还有 N 分钟」会跨秒不一致。
    let now = swarmdrop_host::now_secs();
    for row in rows {
        let state = state(row);
        println!(
            "{}  {state}  {}",
            short(&row.id),
            issued(row.created_at, now)
        );
        println!("   {}   还有 {}", row.id, remaining(row.expires_at, now));
    }

    eprintln!();
    // **必须说明列表里为什么没有链接**：用户会以为是这条命令漏了什么。
    eprintln!("列表不含邀请链接本身——凭证明文不落盘，重启后拼不回来。想再分享请重新生成一张。");
}

/// 选择菜单里的一行。
///
/// `now` 由调用方给：菜单是一次性渲染出整屏的，每行各取一次时钟会让相邻两行
/// 落在不同的秒上（「还有 59 分钟」紧挨着「还有 58 分钟」）。
pub fn menu_line(row: &InviteRow, now: u64) -> String {
    let state = state(row);
    format!(
        "{}  {state}  还有 {}",
        short(&row.id),
        remaining(row.expires_at, now)
    )
}

/// 发出多久了。
///
/// spec「列出已发出的邀请」要求清单给出创建时刻。用**相对时间**而非绝对时刻是刻意的：
/// 这一列要回答的是「哪张是我刚发的、哪张是昨天那张」——一分钟内发两张时，标识前缀之外
/// 就靠它区分，而 `2026-08-19 15:30:07` 在那个用途上比「3 分钟前」难读。
/// 绝对值在 `--json` 里（`createdAt`，Unix 秒），要精确的调用方取那个。
fn issued(created_at: u64, now: u64) -> String {
    match now.saturating_sub(created_at) {
        0..=59 => "刚刚发出".into(),
        left @ 60..=3599 => format!("{} 分钟前发出", left / 60),
        left => format!("{} 小时前发出", left / 3600),
    }
}

/// 剩余有效期的人话。
///
/// `saturating_sub` 而非 `checked_sub` + 单独的 None 分支：已过期时得 0，落进第一档，
/// 与那条分支本来要返回的串一模一样。
fn remaining(expires_at: u64, now: u64) -> String {
    match expires_at.saturating_sub(now) {
        0..=59 => "不到 1 分钟".into(),
        left @ 60..=3599 => format!("{} 分钟", left / 60),
        left => format!("{} 小时", left / 3600),
    }
}

/// 一张邀请的去向。
///
/// 已被使用的仍列到过期为止——用户要能看到「这张被谁用掉了」，而不是它凭空消失。
fn state(row: &InviteRow) -> &'static str {
    if row.consumed {
        "已被使用"
    } else {
        "等待中"
    }
}

/// 撤销了指定的若干张。
///
/// 一张与多张共用这一个函数：分成两个的话，「没落盘」那句警告会有两份，而它们
/// 迟早各说各的——那正是用户唯一能得知「重启后邀请会复活」的地方。
///
/// 结构化形态的 `event` 是 `invitesRevoked`，与 [`render_revoked_all`] 的
/// `allInvitesRevoked` **必须是两个名字**：两者是两个不同的动作（撤指定的几张 vs 连
/// 本命令列不出来的、这一瞬新签发的也一并作废），而 `event` 正是调用方用来分辨动作的
/// 那个字段。共用一个名字会逼脚本去探测 `ids` 在不在——那等于把类型信息藏进字段集合。
pub fn render_revoked(rows: &[InviteRow], outcome: &RevokeOutcome, json: bool) {
    if json {
        let payload = serde_json::json!({
            "event": "invitesRevoked",
            "ids": rows.iter().map(|row| &row.id).collect::<Vec<_>>(),
            "revoked": outcome.revoked,
            "persisted": outcome.persisted,
        });
        println!("{payload}");
        return;
    }

    match rows {
        [only] => println!(
            "已撤销 {}。此后出示这张邀请的配对请求都会被拒绝。",
            short(&only.id)
        ),
        many => {
            // 逐条列出而不是只报个数：用户刚在菜单里勾了几行，这里要能对上账。
            println!("已撤销 {} 张邀请：", many.len());
            for row in many {
                println!("  {}", short(&row.id));
            }
            println!("此后出示它们的配对请求都会被拒绝。");
        }
    }
    warn_if_not_persisted(outcome);
}

/// 撤销了全部。
pub fn render_revoked_all(outcome: &RevokeOutcome, json: bool) {
    if json {
        let payload = serde_json::json!({
            // 与逐张撤销的 `invitesRevoked` **刻意不同名**，见 `render_revoked`。
            "event": "allInvitesRevoked",
            "revoked": outcome.revoked,
            "persisted": outcome.persisted,
        });
        println!("{payload}");
        return;
    }

    println!("已撤销全部 {} 张邀请。", outcome.revoked);
    warn_if_not_persisted(outcome);
}

/// 撤销没落盘时如实说后果。
///
/// **不能报成失败**：撤销这个动作确实完成了，本次运行内它已经生效；没落地的是记录。
/// 但也**不能不说**——用户会以为已经撤销干净了，而重启后那些邀请会复活。
fn warn_if_not_persisted(outcome: &RevokeOutcome) {
    if !outcome.persisted {
        eprintln!(
            "警告：撤销没能写入本机记录，**重启后这些邀请会重新可用**。\n\
             请检查数据目录是否可写，然后再撤一次。"
        );
    }
}

/// 等待对方用这条邀请配对。**写 stderr**：它是过程信息，不是命令结果。
///
/// 三件事必须说到，否则用户会在错误的地方等：这张邀请活多久、待会儿会不会问他、
/// 以及（临时节点时）命令一退出它就废了。
pub fn render_waiting(temporary_node: bool, auto_accept: bool, json: bool) {
    if json {
        return;
    }

    eprintln!();
    eprintln!("等待对方配对…（Ctrl-C 取消）");
    if auto_accept {
        eprintln!("⚠️ 已开启自动接受：第一台出示这张邀请的设备会直接配上，不会再问你。");
    } else {
        eprintln!("有设备请求配对时会在这里列出它的信息，由你确认后才会接受。");
    }

    if temporary_node {
        eprintln!("注意：这张邀请在本命令退出后即失效——它的可拨地址就是当前这个进程的节点。");
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
/// **要说清楚「邀请还能用」**：用户拒掉一个陌生请求后最担心的就是自己那张邀请废了。
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
    eprintln!("已拒绝。这张邀请没有被消耗，仍在等待——对方可以继续用它配对。");
    eprintln!();
}

/// 待确认的请求在确认期间失效了。
pub fn render_request_expired(json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "event": "pairingRequestExpired" })
        );
        return;
    }
    eprintln!("这条配对请求已失效（对方断开或等待超时），仍在等待下一次。");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 清单必须给得出「这张是什么时候发的」——spec「列出已发出的邀请」要求输出含创建时刻。
    ///
    /// 它不是装饰：一分钟内发两张时，标识前缀之外就靠这一列区分哪张发给了谁，
    /// 而那正是「刚发错人、立刻撤回」这条主场景可用的前提。
    #[test]
    fn listing_says_when_each_invite_was_issued() {
        let now = 1_800_000_000;
        assert_eq!(issued(now, now), "刚刚发出");
        assert_eq!(issued(now - 180, now), "3 分钟前发出");
        assert_eq!(issued(now - 7200, now), "2 小时前发出");
        // 时钟回拨不该 panic，也不该给出一个巨大的数。
        assert_eq!(issued(now + 60, now), "刚刚发出");
    }

    /// 已过期的邀请不会进清单（`list_active` 过滤掉了），但渲染仍要能收住。
    #[test]
    fn remaining_never_underflows() {
        let now = 1_800_000_000;
        assert_eq!(remaining(now - 100, now), "不到 1 分钟");
        assert_eq!(remaining(now + 120, now), "2 分钟");
        assert_eq!(remaining(now + 7200, now), "2 小时");
    }
}
