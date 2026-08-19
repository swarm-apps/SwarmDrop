//! 传输记录的渲染。
//!
//! 输入是**已经序列化好的 JSON** 而非核心的 DTO：记录可能来自本机的库，也可能来自通道
//! 对面的常驻节点，后者只能是 JSON。让渲染层统一吃 JSON，就不必为两条来源各写一份。

use serde_json::Value;

use super::{bytes_or_dash as bytes, text_or};

pub fn render_list(records: &Value, json: bool) {
    if json {
        super::emit_json(records, "传输记录");
        return;
    }

    let Some(list) = records.as_array() else {
        println!("（无法解析传输记录）");
        return;
    };

    if list.is_empty() {
        println!("还没有传输记录。");
        return;
    }

    for item in list {
        let direction = direction_glyph(item.get("direction").and_then(Value::as_str));
        println!(
            "{direction} {}  {}",
            text_or(item, "peerName", "—"),
            text_or(item, "phase", "—")
        );
        println!(
            "   {}  {} / {}",
            text_or(item, "sessionId", "—"),
            bytes(item.get("transferredBytes")),
            bytes(item.get("totalSize"))
        );
    }
}

pub fn render_detail(record: &Value, json: bool) {
    if json {
        super::emit_json(record, "传输记录");
        return;
    }

    println!("会话      {}", text_or(record, "sessionId", "—"));
    println!("方向      {}", text_or(record, "direction", "—"));
    println!("对端      {}", text_or(record, "peerName", "—"));
    println!("阶段      {}", text_or(record, "phase", "—"));
    println!(
        "进度      {} / {}",
        bytes(record.get("transferredBytes")),
        bytes(record.get("totalSize"))
    );
    if let Some(failure) = record.get("failure").and_then(Value::as_str) {
        println!("失败原因  {failure}");
    }
    // 可恢复与否决定用户下一步能做什么，值得单列。
    if record.get("recoverable").and_then(Value::as_bool) == Some(true) {
        println!("可恢复    是");
    }
}

/// 传输方向的图标。
///
/// 名字来自 `entity::TransferDirection` 的 serde 形态（`rename_all = "lowercase"`）。
/// 曾经这里还写着 `Some("Send")` / `Some("Receive")` 两个 arm——**它们永不命中**，
/// 是照着想象写的。
///
/// ⚠️ 兜底给「·」意味着核心一改 `rename_all`，**每一行的方向都会静默变成「·」**，
/// 而没有任何东西会报错。`direction_names_match_the_wire` 是唯一的看守。
fn direction_glyph(direction: Option<&str>) -> &'static str {
    match direction {
        Some("send") => "↑",
        Some("receive") => "↓",
        _ => "·",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **render 认识的方向名必须与核心实际序列化出来的一致。**
    ///
    /// 两者是两份独立的字符串：一份写在 `entity::TransferDirection` 的
    /// `#[serde(rename_all = "lowercase")]` 上，一份写在 `direction_glyph` 的 match 里。
    /// 核心改了那个属性、或改了变体名，这里的 match 会全部落进 `_` 兜底——
    /// 传输列表每一行的方向图标静默变成「·」，不报错、不 panic、测试也不红。
    /// 除非有这一条。
    #[test]
    fn direction_names_match_the_wire() {
        let send = serde_json::to_value(entity::TransferDirection::Send).expect("序列化");
        let receive = serde_json::to_value(entity::TransferDirection::Receive).expect("序列化");

        assert_eq!(
            direction_glyph(send.as_str()),
            "↑",
            "发送方向对不上 wire 名"
        );
        assert_eq!(
            direction_glyph(receive.as_str()),
            "↓",
            "接收方向对不上 wire 名"
        );
        // 兜底仍然存在，但它只该接住「真的没有这个字段」。
        assert_eq!(direction_glyph(None), "·");
    }

    #[test]
    fn formats_sizes_in_binary_units() {
        assert_eq!(bytes(Some(&Value::from(512))), "512 B");
        assert_eq!(bytes(Some(&Value::from(1024))), "1.0 KiB");
        assert_eq!(bytes(Some(&Value::from(1024 * 1024 * 3))), "3.0 MiB");
    }

    /// 缺字段要给占位符，不能 panic 也不能打印 `null`——记录可能来自旧版本的库。
    #[test]
    fn missing_fields_degrade_gracefully() {
        assert_eq!(bytes(None), "—");
        assert_eq!(text_or(&serde_json::json!({}), "peerName", "—"), "—");
    }
}
