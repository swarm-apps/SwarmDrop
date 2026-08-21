//! 设置的两套渲染。
//!
//! **人类可读那套的重点是「为什么是这个值」而不是值本身**：一个只显示值的界面，会让用户
//! 在环境变量存在时反复修改一个不生效的输入框，而界面无从解释为什么。

use crate::runtime::settings::scalar::{Effect, ScalarView, ScalarWritten, Source};

/// 值缺席时的占位符。
///
/// 只在一种情形下出现：这一项既没被设过，本机又给不出内置默认（拿不到下载目录的
/// 无桌面环境）。**不能显示成空白**——那与「值是空串」看起来一模一样。
const UNSET: &str = "（未设置）";

pub fn render_list(views: &[ScalarView], json: bool) {
    if json {
        super::emit_json(views, "配置");
        return;
    }

    for view in views {
        println!(
            "{:<14}{}",
            view.key.as_str(),
            view.value.as_deref().unwrap_or(UNSET)
        );
        if let Some(note) = origin_note(view) {
            println!("{:<14}{note}", "");
        }
    }
}

/// 单项：**值走 stdout，来源走 stderr。**
///
/// 这样 `NAME=$(swarmdrop config get device-name)` 拿到的是干净的一行，而用户在终端里
/// 仍然看得到来源说明（spec: cli-host 的流向约束——结果归 stdout，诊断归 stderr）。
pub fn render_one(view: &ScalarView, json: bool) {
    if json {
        super::emit_json(view, "配置项");
        return;
    }

    println!("{}", view.value.as_deref().unwrap_or(UNSET));
    if let Some(note) = origin_note(view) {
        eprintln!("{note}");
    }
}

pub fn render_written(written: &ScalarWritten, json: bool) {
    if json {
        super::emit_json(written, "写入结果");
        return;
    }

    let key = written.view.key.as_str();
    match &written.view.configured {
        Some(value) => println!("已设置 {key} = {value}"),
        // 清除之后要把**回落到的那个值**说出来：不说的话，用户不知道自己现在归到了哪。
        None => println!(
            "已清除 {key}，回落到 {}",
            written.view.value.as_deref().unwrap_or(UNSET)
        ),
    }

    // 生效状态走 stderr：它是对这次动作的说明，不是动作的结果。
    eprintln!("{}", effect_note(&written.effect));
}

/// 这个值为什么是这个值。已经是「用户设的、且此刻生效」时返回 `None`——
/// 正常情况下不该多说一句。
fn origin_note(view: &ScalarView) -> Option<String> {
    match view.source {
        Source::Config => None,
        Source::Default => Some("（默认值。用 swarmdrop config set 改它）".to_owned()),
        Source::Env => {
            let var = view.overridden_by.as_deref().unwrap_or("环境变量");
            Some(match view.configured.as_deref() {
                // **被压住的那个值必须说出来**，否则用户看不出自己设过什么。
                Some(configured) => {
                    format!("（来自 {var}；配置里的 {configured} 此刻不生效）")
                }
                None => format!("（来自 {var}）"),
            })
        }
    }
}

/// 这次写入此刻算不算数。
fn effect_note(effect: &Effect) -> String {
    match effect {
        Effect::Applied => "已生效，无需重启节点。".to_owned(),
        Effect::PendingStart => "已保存；下次启动节点时生效。".to_owned(),
        Effect::Overridden { by } => {
            format!("已保存，但此刻被环境变量 {by} 压着，不生效。取消它之后才会用上这个值。")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::settings::scalar::ScalarKey;

    fn view(source: Source, configured: Option<&str>) -> ScalarView {
        ScalarView {
            key: ScalarKey::ReceiveDir,
            value: Some("/tmp/effective".into()),
            source,
            configured: configured.map(str::to_owned),
            overridden_by: matches!(source, Source::Env)
                .then(|| "SWARMDROP_RECEIVE_DIR".to_owned()),
        }
    }

    /// **被环境变量压住时，配置里那个值必须出现在说明里。**
    ///
    /// 这条是整个读面存在的理由：只说「当前值来自环境变量」，用户仍然看不出自己设过
    /// 什么、也就不知道取消环境变量之后会变成什么。
    #[test]
    fn an_overridden_value_names_both_the_variable_and_what_it_hides() {
        let note = origin_note(&view(Source::Env, Some("/tmp/configured"))).expect("要有说明");
        assert!(note.contains("SWARMDROP_RECEIVE_DIR"), "{note}");
        assert!(note.contains("/tmp/configured"), "{note}");
    }

    /// 用户自己设的、且此刻生效——不该多说一句。
    #[test]
    fn a_plain_configured_value_says_nothing_extra() {
        assert!(origin_note(&view(Source::Config, Some("/tmp/effective"))).is_none());
    }

    /// 三种生效状态各说各的，且都不含「重启节点」这个指示——本宿主任何一项都不需要它。
    #[test]
    fn every_effect_has_its_own_wording() {
        let notes = [
            effect_note(&Effect::Applied),
            effect_note(&Effect::PendingStart),
            effect_note(&Effect::Overridden {
                by: "SWARMDROP_RECEIVE_DIR".into(),
            }),
        ];
        for note in &notes {
            assert!(!note.is_empty());
        }
        assert_eq!(
            notes.iter().collect::<std::collections::HashSet<_>>().len(),
            3,
            "三种状态的措辞不得重复——调用方与用户都靠它区分"
        );
    }
}
