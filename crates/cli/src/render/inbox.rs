//! 收件箱渲染。
//!
//! 输入是 JSON 而非具体 DTO：数据可能来自本进程直连的数据库，也可能来自通道对面的
//! 常驻节点，后者只能是 JSON。统一吃 JSON 就不必为两条来源各写一份渲染。

use std::path::Path;

use serde_json::Value;

pub fn render_list(items: &Value, json: bool) {
    if json {
        emit(items);
        return;
    }

    let Some(list) = items.as_array() else {
        println!("（无法解析收件箱）");
        return;
    };
    if list.is_empty() {
        println!("收件箱是空的。");
        return;
    }

    for item in list {
        let missing = item.get("missing").and_then(Value::as_bool) == Some(true);
        println!(
            "{} {}",
            if missing { "!" } else { " " },
            text(item, "title")
        );
        println!("   {}  来自 {}", text(item, "id"), text(item, "sourceName"));
        println!(
            "   {} 项 · {}",
            item.get("itemCount").and_then(Value::as_i64).unwrap_or(0),
            crate::render::send::human_bytes(
                item.get("totalSize")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0) as u64
            )
        );
    }
}

pub fn render_detail(detail: &Value, json: bool) {
    if json {
        emit(detail);
        return;
    }

    println!("标题    {}", text(detail, "title"));
    println!("来自    {}", text(detail, "sourceName"));
    println!("标识    {}", text(detail, "id"));

    if detail.pointer("/content/kind").and_then(Value::as_str) == Some("text") {
        println!("内容    文本");
        println!(
            "{}",
            detail
                .pointer("/content/body")
                .and_then(Value::as_str)
                .unwrap_or("")
        );
        return;
    }

    let entries = detail
        .pointer("/content/entries")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    println!("内容    {} 个文件", entries.len());
    for entry in entries {
        let missing = entry.get("missing").and_then(Value::as_bool) == Some(true);
        println!(
            "{}  {}  {}",
            if missing { "!" } else { " " },
            text(entry, "relativePath"),
            crate::render::send::human_bytes(
                entry
                    .get("size")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .max(0) as u64
            )
        );
    }
}

pub fn render_exported(count: usize, to: &Path, json: bool) {
    if json {
        let payload = serde_json::json!({ "exported": count, "to": to.to_string_lossy() });
        println!("{payload}");
    } else {
        println!("已导出 {count} 项到 {}", to.display());
    }
}

/// 结构化输出。序列化失败时什么都不写——stdout 只能有完整结果。
fn emit(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("序列化失败: {err}"),
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("—")
        .to_owned()
}
