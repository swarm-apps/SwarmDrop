//! 收件箱渲染。

use std::path::Path;

use swarmdrop_core::transfer::inbox::{InboxItemContent, InboxItemDetail, InboxItemSummary};

pub fn render_list(items: &[InboxItemSummary], json: bool) {
    if json {
        emit_json(items);
        return;
    }

    if items.is_empty() {
        println!("收件箱是空的。");
        return;
    }

    for item in items {
        let mark = if item.missing { "!" } else { " " };
        println!("{mark} {}", item.title);
        println!("   {}  来自 {}", item.id, item.source_name);
        println!(
            "   {} 项 · {}",
            item.item_count,
            human_bytes(item.total_size)
        );
    }
}

pub fn render_detail(detail: &InboxItemDetail, json: bool) {
    if json {
        emit_json(detail);
        return;
    }

    println!("标题    {}", detail.item.title);
    println!("来自    {}", detail.item.source_name);
    println!("标识    {}", detail.item.id);

    match &detail.content {
        InboxItemContent::Files { entries, .. } => {
            println!("内容    {} 个文件", entries.len());
            for entry in entries {
                let mark = if entry.missing { "!" } else { " " };
                println!(
                    "{mark}  {}  {}",
                    entry.relative_path,
                    human_bytes(entry.size)
                );
            }
        }
        InboxItemContent::Text { body } => {
            println!("内容    文本");
            println!("{body}");
        }
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
fn emit_json<T: serde::Serialize + ?Sized>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(err) => eprintln!("序列化失败: {err}"),
    }
}

/// 人类可读的字节数（二进制单位，与文件管理器显示一致）。
fn human_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return "?".into();
    }
    crate::render::send::human_bytes(bytes as u64)
}
