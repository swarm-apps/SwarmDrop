//! 收件箱渲染。
//!
//! 输入是 JSON 而非具体 DTO：数据可能来自本进程直连的数据库，也可能来自通道对面的
//! 常驻节点，后者只能是 JSON。统一吃 JSON 就不必为两条来源各写一份渲染。

use std::path::Path;

use serde_json::Value;

use super::{bytes_or_dash, flag, int_or_zero, text_or};

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
        let missing = flag(item, "missing");
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
            int_or_zero(item, "itemCount"),
            bytes_or_dash(item.get("totalSize"))
        );
    }
}

/// 选择菜单里的一行。
///
/// 标题 + 来源 + 件数大小，够用户认出是哪一条；标识（UUID）刻意不进来——36 个字符会
/// 把真正能区分的信息挤出屏幕，而用户认不出一串随机十六进制是哪次接收。
///
/// **文件缺失的标记要留着**：那条记录仍在收件箱里，但导出它只会得到一个空目录，
/// 用户有权在选之前就看见。
pub fn menu_line(item: &Value) -> String {
    let missing = flag(item, "missing");
    format!(
        "{}{}  来自 {}  {} 项 · {}",
        if missing { "! " } else { "" },
        text_or(item, "title", "—"),
        text_or(item, "sourceName", "—"),
        int_or_zero(item, "itemCount"),
        bytes_or_dash(item.get("totalSize"))
    )
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
        let missing = flag(entry, "missing");
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
