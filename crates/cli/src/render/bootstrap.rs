//! 引导节点清单与增删的渲染。

use swarmdrop_core::infra::{InfraLink, RelayLinkState};

use crate::runtime::bootstrap_nodes::{BootstrapChanged, BootstrapRow, Origin};
use crate::runtime::settings::scalar::Effect;

/// 连接状态的标记。
///
/// **三态而非两态**：`·` 是「未知」（本机没启动节点，从未探测过），把它显示成「连不上」
/// 会让用户去排查网络，而真实原因是节点没开。与 `render::device` 同一套记号。
fn presence(row: &BootstrapRow) -> &'static str {
    match &row.link {
        Some(link) if link.connected => "●",
        Some(_) => "○",
        None => "·",
    }
}

fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::Builtin => "内置",
        Origin::Custom => "自定义",
    }
}

/// 中继那条轨道此刻怎么样。
///
/// `last_error` **原样带出内核下发的字符串**——这是唯一能说清「为什么连不上」的东西，
/// 排查时用户要贴的就是这一句，不翻译、不改写。
fn relay_note(link: &InfraLink) -> Option<String> {
    match &link.relay {
        Some(RelayLinkState::Connecting) => Some("中继  连接中".to_owned()),
        Some(RelayLinkState::Active { circuit_addr }) => {
            Some(format!("中继  已就绪（{circuit_addr}）"))
        }
        Some(RelayLinkState::Failed { last_error }) => Some(format!("中继  连不上：{last_error}")),
        // 不承担中继角色，或被「公网可达性」开关拦下——两者都不是故障，不必说。
        None => None,
    }
}

pub fn render_list(rows: &[BootstrapRow], json: bool) {
    if json {
        super::emit_json(rows, "引导节点清单");
        return;
    }

    if rows.is_empty() {
        // **清空到零条是允许的**，所以这里不是「出错了」而是一句陈述 + 后果。
        println!("生效清单里没有引导节点。");
        eprintln!("本机将只能在局域网内发现设备；跨网需要至少一条引导 / 中继节点。");
        eprintln!("用 swarmdrop bootstrap add <multiaddr> 加一条。");
        return;
    }

    for row in rows {
        println!("{} {}", presence(row), row.addr);
        println!("  {}", origin_label(row.origin));
        if let Some(note) = row.link.as_ref().and_then(relay_note) {
            println!("  {note}");
        }
    }

    if rows.iter().any(|row| row.link.is_none()) {
        eprintln!();
        eprintln!("· = 连接状态未知（节点未运行，本机没有探测过）。执行 swarmdrop start 后再看。");
    }
}

/// 选择菜单里的一行。要够用户分辨出是哪一条。
pub fn menu_line(row: &BootstrapRow) -> String {
    format!("{} [{}]", row.addr, origin_label(row.origin))
}

pub fn render_added(changed: &BootstrapChanged, json: bool) {
    if json {
        super::emit_json(changed, "添加结果");
        return;
    }
    println!("已添加引导节点：{}", changed.addr);
    eprintln!("{}", effect_note(&changed.effect, changed.remaining));
}

pub fn render_removed(changed: &BootstrapChanged, json: bool) {
    if json {
        super::emit_json(changed, "撤销结果");
        return;
    }
    println!("已撤销引导节点：{}", changed.addr);
    eprintln!("{}", effect_note(&changed.effect, changed.remaining));
}

/// 这次改动此刻算不算数，以及改完还剩几条。
///
/// **剩零条要单独说**：那是允许的状态，但它的后果（跨网连不上）用户必须当场知道，
/// 而不是过几天发现「怎么突然只能在局域网里用了」。
fn effect_note(effect: &Effect, remaining: usize) -> String {
    let when = match effect {
        Effect::Applied => "已生效，无需重启节点。",
        Effect::PendingStart => "已保存；下次启动节点时生效。",
        // 引导清单没有环境变量覆盖，这一支不会出现；措辞仍要成立，不写 unreachable。
        Effect::Overridden { .. } => "已保存，但此刻被环境变量压着。",
    };

    if remaining == 0 {
        format!(
            "{when}\n生效清单已空——本机将只能在局域网内发现设备，跨网不可达。\n\
             用 swarmdrop bootstrap add <multiaddr> 加回一条。"
        )
    } else {
        format!("{when}生效清单里还有 {remaining} 条。")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **清空到零条必须把后果说出来。**
    ///
    /// 它是一个合法状态（用户可能只在局域网内用），所以写入路径不拦它——那就只剩这里
    /// 有机会告诉用户他刚做了什么。
    #[test]
    fn emptying_the_list_spells_out_the_consequence() {
        let note = effect_note(&Effect::Applied, 0);
        assert!(note.contains("局域网"), "{note}");
        assert!(note.contains("bootstrap add"), "{note}");
    }

    /// 还剩若干条时报个数就够，不必吓唬人。
    #[test]
    fn a_non_empty_list_just_reports_the_count() {
        let note = effect_note(&Effect::PendingStart, 2);
        assert!(note.contains("还有 2 条"), "{note}");
        assert!(!note.contains("局域网"), "{note}");
    }
}
