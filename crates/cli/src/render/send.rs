//! 发送结果渲染。

use crate::runtime::transfer::SendOutcome;

pub fn render(outcome: &SendOutcome, json: bool) {
    if json {
        let payload = serde_json::json!({
            "sessionId": outcome.session_id.to_string(),
            "fileCount": outcome.file_count,
            "totalBytes": outcome.total_bytes,
        });
        println!("{payload}");
    } else {
        println!(
            "已发送 {} 个文件（{}）",
            outcome.file_count,
            human_bytes(outcome.total_bytes)
        );
    }
}

/// 结果来自通道对面的常驻节点时，它已经是 JSON。
pub fn render_from_json(payload: &serde_json::Value, json: bool) {
    if json {
        println!("{payload}");
        return;
    }
    let count = payload
        .get("fileCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let bytes = payload
        .get("totalBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    println!("已发送 {count} 个文件（{}）", human_bytes(bytes));
}

/// 人类可读的字节数。
///
/// 用 1024 进制并标注 KiB/MiB：文件大小在磁盘与传输语境下都是二进制单位，
/// 用 1000 进制会与用户在文件管理器里看到的数对不上。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 传输进度。**写 stderr**：结构化模式下 stdout 只能有最终结果，
/// 而人类可读模式下把进度和结果混在一起也不利于管道使用。
///
/// 用回车原地刷新而非逐行追加：一次传输会有成百上千条进度事件，逐行打印会把终端刷满。
pub fn render_progress(transferred: u64, total: u64) {
    use std::io::Write;

    let percent = if total == 0 {
        0.0
    } else {
        transferred as f64 / total as f64 * 100.0
    };
    eprint!(
        "\r传输中 {percent:>5.1}%  {} / {}",
        human_bytes(transferred),
        human_bytes(total)
    );
    let _ = std::io::stderr().flush();
}

/// 结束进度行——否则最终结果会接在那条没有换行的进度后面。
pub fn finish_progress() {
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    }
}
