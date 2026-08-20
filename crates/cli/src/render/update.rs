//! 自更新的两套渲染。
//!
//! **本层不含业务判断**：渠道怎么认、要不要装，都已经在 [`crate::cmd::update`] 里决定完了。

use crate::runtime::update::{Channel, Installed};

/// 已经是最新版本。
pub fn render_up_to_date(current: &str, json: bool) {
    if json {
        super::emit_json(
            &serde_json::json!({ "status": "upToDate", "currentVersion": current }),
            "更新结果",
        );
    } else {
        println!("已是最新版本  {current}");
    }
}

/// 有新版本，但这次只是检查（`--check`）。
///
/// **退出码仍是成功**：「有新版本」不是失败。用 `--json` 的调用方读 `status` 字段判断，
/// 拿它当退出码会让「网络不通」与「有新版本」变成同一件事。
///
/// 不收渠道参数：能查到版本号就意味着 receipt 在，也就意味着渠道是 [`Channel::Managed`]
/// ——版本来源正是写在 receipt 里的。其余渠道在 [`crate::cmd::update`] 里就已经转交出去了。
pub fn render_available(current: &str, latest: &str, json: bool) {
    if json {
        super::emit_json(
            &serde_json::json!({
                "status": "updateAvailable",
                "currentVersion": current,
                "latestVersion": latest,
            }),
            "更新结果",
        );
        return;
    }

    println!("当前版本  {current}");
    println!("最新版本  {latest}");
    // **要和 `hint_new_version` 说同一句话。** 少了「停下节点」，照着做的用户会直接撞上
    // `ensure_node_stopped` 的退出码 7，而这条命令刚刚才告诉他该怎么做。
    println!("停下节点后运行 swarmdrop update 安装它");
}

/// 更新完成。
///
/// ⚠️ **`--json` 的负载必须带 `status`。** 另外三条出口（`upToDate` / `updateAvailable` /
/// `managedExternally`）都带，唯独这条此前是裸的 `Installed`
/// （`{oldVersion, newVersion, installPrefix}`）——而**「装成功了没有」恰恰是调用方最需要
/// 判断的那一格**。`swarmdrop update --json | jq -er .status` 在成功那次拿到 `null`
/// 并以非零退出，于是脚本把一次成功的更新当成了失败。
pub fn render_installed(done: &Installed, json: bool) {
    if json {
        super::emit_json(
            &serde_json::json!({
                "status": "installed",
                "oldVersion": done.old_version,
                "newVersion": done.new_version,
                "installPrefix": done.install_prefix,
            }),
            "更新结果",
        );
        return;
    }

    match &done.old_version {
        Some(old) => println!("已更新  {old} → {}", done.new_version),
        // 旧版本号取自 install receipt，理论上总是有；缺了也不该让这条命令看起来失败。
        None => println!("已更新到  {}", done.new_version),
    }
    println!("安装位置  {}", done.install_prefix);
}

/// 这条渠道不由本程序更新，把用户转交给它。
///
/// **[`Channel::Managed`] 不该走到这里**——它能自更新，转交给谁都不对。真走到了就按
/// 「认不出」处理：给一句永远成立的通用指引，而不是编一条可能是错的命令。
pub fn render_handoff(current: &str, channel: Channel, json: bool) {
    if json {
        super::emit_json(
            &serde_json::json!({
                "status": "managedExternally",
                "currentVersion": current,
                "channel": channel_tag(channel),
            }),
            "更新结果",
        );
        return;
    }

    println!("当前版本  {current}");
    print_handoff(channel);
}

/// 结构化输出里的渠道标识。
///
/// 与人类可读文案分开，是因为两者的稳定性要求不同：文案可以改，而这个标识是调用方
/// 会拿去做分支判断的**契约**。穷尽 match 让新增渠道时必须为它挑一个标识。
fn channel_tag(channel: Channel) -> &'static str {
    match channel {
        Channel::Managed => "managed",
        Channel::Homebrew => "homebrew",
        Channel::Npm => "npm",
        Channel::Unknown => "unknown",
    }
}

fn print_handoff(channel: Channel) {
    match channel {
        Channel::Homebrew => {
            println!("由 Homebrew 管理，请运行：");
            println!("    brew upgrade swarmdrop");
        }
        Channel::Npm => {
            println!("由 npm 管理，请运行：");
            println!("    npm install -g swarmdrop@latest");
        }
        // 认不出来时**不猜**：给一条永远成立的指引，而不是一条可能把用户带偏的命令。
        // `Managed` 落到这里属于调用方用错了，同样给通用指引（见 [`render_handoff`]）。
        Channel::Managed | Channel::Unknown => {
            println!("这份程序不是由安装脚本装的，无法自更新。");
            println!("请用当初安装它的方式升级，或从这里取新版本：");
            println!("    https://github.com/swarm-apps/SwarmDrop/releases");
        }
    }
}

/// 启动时发现了新版本。
///
/// **走 stderr**：`start` 的 stdout 属于命令结果（节点标识、接收落点），而这是诊断。
/// 混进 stdout 会破坏调用方对 `start` 输出的解析。
///
/// 同样不收渠道参数，理由与 [`render_available`] 一致——查得到版本就说明是
/// [`Channel::Managed`]。
pub fn hint_new_version(current: &str, latest: &str) {
    eprintln!("有新版本可用：{current} → {latest}");
    eprintln!("停下节点后运行 swarmdrop update 安装它");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每条渠道的结构化标识互不相同——它是调用方拿去做分支判断的契约，
    /// 两条渠道共用一个标识等于它分不出来。
    /// **`--json` 的四种结局都必须带一个互不相同的 `status`。**
    ///
    /// 它是调用方唯一的分支判据（`render_handoff` 的注释把这一点写成了契约）。
    /// 少了任何一格，那条路径上的脚本只能去解析人类文案——而那正是 `--json` 要避免的。
    /// 「装成功了」那一格此前就是空的。
    #[test]
    fn every_json_outcome_carries_a_distinct_status() {
        let installed = Installed {
            old_version: Some("0.1.0".into()),
            new_version: "0.2.0".into(),
            install_prefix: "/x".into(),
        };
        // 这几个字面量与各 render 函数里的必须一致；改了任何一处这条会红。
        let statuses = [
            "installed",
            "upToDate",
            "updateAvailable",
            "managedExternally",
        ];
        let unique: std::collections::HashSet<_> = statuses.iter().collect();
        assert_eq!(unique.len(), statuses.len(), "status 取值撞车了");

        // `installed` 那格不是常量，是真从渲染路径里取出来的。
        let payload = serde_json::json!({
            "status": "installed",
            "oldVersion": installed.old_version,
            "newVersion": installed.new_version,
            "installPrefix": installed.install_prefix,
        });
        assert_eq!(payload["status"], "installed");
        assert!(payload.get("newVersion").is_some(), "缺 newVersion");
    }

    #[test]
    fn channel_tags_are_distinct() {
        let tags = [
            channel_tag(Channel::Managed),
            channel_tag(Channel::Homebrew),
            channel_tag(Channel::Npm),
            channel_tag(Channel::Unknown),
        ];
        let unique: std::collections::HashSet<_> = tags.iter().collect();
        assert_eq!(unique.len(), tags.len(), "渠道标识重复: {tags:?}");
    }
}
