//! 输出渲染：人类可读与机器可读两套。
//!
//! **本层不含业务判断**。它只把已经算好的结果变成字节。任何「要不要显示」「显示哪个」
//! 的决定都属于 [`crate::cmd`]。
//!
//! ## 流向是硬约束
//!
//! | 内容 | 去向 |
//! |---|---|
//! | 结构化结果（`--json`） | **stdout** |
//! | 人类可读结果 | stdout |
//! | 进度、状态行、诊断、日志 | **stderr** |
//!
//! 结构化模式下 stdout 只能有最终结果：混入任何一行进度都会破坏调用方的解析
//! （spec: cli-host「结构化输出模式」）。

pub mod device;
pub mod inbox;
pub mod invite;
pub mod send;
pub mod status;
pub mod transfer;
pub mod update;
pub mod watch;

/// 把结构化结果写到 stdout。
///
/// **序列化失败时什么都不写**：stdout 在结构化模式下只能有完整结果，写半截比不写更糟——
/// 调用方会拿到一段解析不了的 JSON，而它无法与「命令没有输出」区分。诊断走 stderr。
///
/// 六个渲染模块曾各有一份这四行。
pub fn emit_json<T: serde::Serialize + ?Sized>(value: &T, what: &str) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("序列化{what}失败: {err}"),
    }
}

/// 从一段 JSON 里取一个字段的可读文本，缺失时用 `fallback`。
///
/// 渲染层吃的是 JSON 而非 DTO（数据可能来自本进程，也可能来自通道对面的常驻节点），
/// 于是「取一个字段并降级」这件事每个渲染模块都要做。曾经是三份各自的私有副本，
/// 而三份的行为并不一致——两份把非字符串 `to_string()`、第三份直接当缺失处理，
/// 占位符也分成了「未知」和「—」两种。占位符确实该由调用点决定（状态字段缺失是
/// 「未知」，清单字段缺失是「—」），但**取值与降级的规则不该有第二种**。
///
/// 非字符串走 `to_string()` 而不是当作缺失：核心的枚举序列化后可能是字符串，也可能是
/// 带标签的对象（`natStatus` 就是），当作缺失会把一个本来有值的字段显示成占位符。
pub fn text_or(value: &serde_json::Value, key: &str, fallback: &'static str) -> String {
    match value.get(key) {
        Some(serde_json::Value::String(text)) => text.clone(),
        // **`null` 走占位符**：字段存在但是空，与字段不存在对用户是同一件事。
        // 少了这条，`Option` 字段会渲染成字面量 `null`（「来自 null」）。
        Some(serde_json::Value::Null) | None => fallback.into(),
        Some(other) => other.to_string(),
    }
}

/// 从一段 JSON 里取字节数并转成人话；字段缺失或不是数字时给占位符。
///
/// **缺失给「—」而不是「0 B」**：后者是一个断言（「大小是零」），会让用户以为自己收到了
/// 一个空文件。两者曾经在收件箱与传输列表里各行其是。
pub fn bytes_or_dash(value: Option<&serde_json::Value>) -> String {
    match value.and_then(serde_json::Value::as_i64) {
        Some(n) => crate::render::send::human_bytes(n.max(0) as u64),
        None => "—".into(),
    }
}

/// 从一段 JSON 里取一个布尔标志；缺失或不是布尔时为假。
///
/// 与 [`text_or`] 同一条理由：**取值与降级的规则不该有第二种**。`missing` 这个标志
/// 此前在收件箱的清单、菜单行、详情与导出里各写了一遍
/// `get(..).and_then(Value::as_bool) == Some(true)`，四份行为一致纯属巧合。
///
/// 缺失当假：这个标志表达的都是「出了点问题」（文件丢了），而**默认不该无中生有地
/// 报警**——旧版本的记录里没有这个字段。
pub fn flag(value: &serde_json::Value, key: &str) -> bool {
    value.get(key).and_then(serde_json::Value::as_bool) == Some(true)
}

/// 从一段 JSON 里取一个计数；缺失或不是数字时为 0。
pub fn int_or_zero(value: &serde_json::Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

/// 标识的短形式。
///
/// 给人看的截断，**按字符不按字节**——节点标识是 base58（ASCII），但这个函数也用在
/// 别处，而按字节切中文会 panic。
///
/// 长度由调用方给：邀请标识按前缀撤销（要够长到唯一），节点标识只用于辨认（短些即可）。
pub fn short(id: &str, chars: usize) -> String {
    id.chars().take(chars).collect()
}

/// 设备名为空时的占位符。
///
/// `OsInfo::display_name` 的文档把占位符留给视图层决定——对端可以报一个空名字，
/// 而打印一个空白比打印占位符更难判断出了什么事。
pub fn blank_as_placeholder(name: &str) -> &str {
    if name.trim().is_empty() {
        "（未命名设备）"
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_field_uses_the_callers_placeholder() {
        assert_eq!(text_or(&json!({}), "status", "未知"), "未知");
        assert_eq!(text_or(&json!({}), "peerName", "—"), "—");
    }

    /// 带标签的枚举对象必须原样出来，不能被当成「缺失」。
    ///
    /// 这是三份副本里那份最严格的实现会犯的错：`natStatus` 序列化成对象时，
    /// 用 `as_str()` 取值会得到 `None`，于是一个有值的字段显示成占位符。
    /// `null` 与「字段不存在」对用户是同一件事，都该给占位符。
    ///
    /// 合并三份私有实现时差点丢掉这条：其中一份用 `as_str().unwrap_or(..)`（`null` 走
    /// 占位符），另两份用 `other.to_string()`（`null` 渲染成字面量 `"null"`）。
    /// 今天经它渲染的字段恰好都不是 `Option`，所以打不出来——但给某个字段加上 `Option`
    /// 的那天，用户会看到一行「来自 null」。
    #[test]
    fn null_is_treated_as_absent() {
        assert_eq!(text_or(&json!({ "peerName": null }), "peerName", "—"), "—");
    }

    #[test]
    fn non_string_values_are_rendered_not_dropped() {
        let value = json!({ "natStatus": { "kind": "public" }, "count": 3 });
        assert!(text_or(&value, "natStatus", "未知").contains("public"));
        assert_eq!(text_or(&value, "count", "—"), "3");
    }

    #[test]
    fn blank_names_get_a_placeholder() {
        assert_eq!(blank_as_placeholder("  "), "（未命名设备）");
        assert_eq!(blank_as_placeholder("study-mac"), "study-mac");
    }
}
