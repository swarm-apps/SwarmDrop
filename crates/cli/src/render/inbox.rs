//! 收件箱渲染。
//!
//! 输入是 JSON 而非具体 DTO：数据可能来自本进程直连的数据库，也可能来自通道对面的
//! 常驻节点，后者只能是 JSON。统一吃 JSON 就不必为两条来源各写一份渲染。

use std::path::Path;

use serde_json::Value;

use super::{bytes_or_dash, text_or};

pub fn render_list(items: &Value, json: bool) {
    if json {
        super::emit_json(items, "收件箱");
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
            text_or(item, "title", "—")
        );
        println!(
            "   {}  来自 {}",
            text_or(item, "id", "—"),
            text_or(item, "sourceName", "—")
        );
        println!(
            "   {} 项 · {}",
            item.get("itemCount").and_then(Value::as_i64).unwrap_or(0),
            bytes_or_dash(item.get("totalSize"))
        );
    }
}

pub fn render_detail(detail: &Value, json: bool) {
    if json {
        super::emit_json(detail, "条目详情");
        return;
    }

    println!("标题    {}", text_or(detail, "title", "—"));
    println!("来自    {}", text_or(detail, "sourceName", "—"));
    println!("标识    {}", text_or(detail, "id", "—"));

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
            text_or(entry, "relativePath", "—"),
            bytes_or_dash(entry.get("size"))
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
