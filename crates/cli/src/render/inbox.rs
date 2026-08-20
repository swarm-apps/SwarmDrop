//! 收件箱渲染。
//!
//! 输入是 JSON 而非具体 DTO：数据可能来自本进程直连的数据库，也可能来自通道对面的
//! 常驻节点，后者只能是 JSON。统一吃 JSON 就不必为两条来源各写一份渲染。

use std::path::Path;
use swarmdrop_core::transfer::inbox::LocalLocation;

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
        println!("{} {}", if missing { "!" } else { " " }, title_line(item));
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
        title_line(item),
        text_or(item, "sourceName", "—"),
        int_or_zero(item, "itemCount"),
        bytes_or_dash(item.get("totalSize"))
    )
}

/// 收件箱标题压成**恰好一行**。
///
/// 文本条目的标题是正文的前 160 字节（`text_delivery::text_preview`），
/// 于是它**带着正文里的换行**——而这里的三个消费方都假定标题占一行：列表把标识与来源
/// 打在下一行、菜单一项就是一行、详情是对齐的两列。原样打出来的结果是列表看起来
/// 散架、菜单的行数与项数对不上（dialoguer 按项计数、按行绘制，选中高亮会错位）。
///
/// 文件条目的标题是文件名，本来就没有换行，这个函数对它是恒等的。
/// 需要原样的正文时看 `swarmdrop inbox show`——那里逐字打印，不经过这里。
fn title_line(item: &Value) -> String {
    let title = text_or(item, "title", "—");

    // **判据是「有没有控制字符」，不只是换行。**
    //
    // 文本条目的标题是**对端发来的正文**——这是本 crate 第一次把远端可控的字符串画进
    // 终端和 dialoguer 菜单。只挡换行的话，`\x1b[2J`（清屏）、`\x1b[A`（上移一行）、
    // `\x07`（响铃）会原样透出并在菜单重绘时**执行**，结果与这个函数要防的症状一模一样：
    // 菜单错位、选中高亮跑到别的行上。截断还可能正好切在转义序列中间，留一个悬空的 `ESC[`。
    //
    // **只在真有控制字符时才压空白**：否则文件名里的连续空格会被悄悄改写，
    // 而那是记录的真实内容。
    let flattened = if title.chars().any(char::is_control) {
        title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            // 空白之外的控制字符（转义序列的 `ESC`、响铃）`split_whitespace` 管不着，
            // 单独滤掉。
            .chars()
            .filter(|c| !c.is_control())
            .collect()
    } else {
        title
    };

    let mut chars = flattened.chars();
    let head: String = chars.by_ref().take(TITLE_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// 标题在一行里最多占几个字符。
///
/// **按字符数而不是显示列宽**：中日韩字符占两列，精确算宽要引入字符宽度表，而这里要的
/// 只是「别把菜单撑到折行」。48 个字符在最坏情况（全中文）下约 96 列——比 80 列宽，
/// 但菜单折一行仍然可读，比引一个表划算。
const TITLE_MAX_CHARS: usize = 48;

pub fn render_detail(detail: &Value, json: bool) {
    if json {
        super::emit_json(detail, "条目详情");
        return;
    }

    println!("标题    {}", title_line(detail));
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
    // **收到的东西在哪儿，是这条命令最该回答的问题。** 命令行端没有「在文件管理器中显示」
    // 那个按钮，路径本身就是答案；而接收落点可以被 `SWARMDROP_RECEIVE_DIR` 改掉、
    // 也只在 `swarmdrop start` 启动那一刻打印过一次，事后无从查起。
    println!("位置    {}", item_location(detail, entries));
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

/// 这条收件箱条目在本地的位置。
///
/// 优先级与桌面端的 `item_target_path`（`src-tauri/src/commands/inbox.rs`）**必须一致**：
/// 单文件条目取那个文件自身的完整路径（用户接着要 `cat` / `cp` 它），多文件条目取
/// `rootPath`——core 已经把它解析成**真实容器目录**（各文件 `local_dir` 一致时就是那个
/// 目录，否则回退存储根），不是靠字符串拼出来的。
///
/// **绝不在这里做「根目录 + 相对路径」的拼接**：各文件的 `local_dir` 未必相同，拼出来的
/// 路径可能根本不存在，而它看起来完全正常（判据写在
/// `swarmdrop_transfer::store::content_root_of`）。
///
/// 文件已缺失时照样给路径——用户正需要它去排查东西被谁挪走了，这一点也与桌面端的
/// 「复制路径」一致（那边同样不校验存在性）。
fn item_location(detail: &Value, entries: &[Value]) -> String {
    // **判据来自领域模型，取字段留在这里**：本层吃的是 JSON（两条取数路径统一成它），
    // 而桌面后端吃 typed DTO——共享的只能是「该取哪个」，不是「怎么取」。
    match swarmdrop_core::transfer::inbox::local_location(entries.len()) {
        LocalLocation::Entry(n) => entries
            .get(n)
            .map_or_else(|| "—".to_owned(), |entry| text_or(entry, "localPath", "—")),
        LocalLocation::Root => text_or(detail, "rootPath", "—"),
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
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// **位置的优先级必须与桌面端的 `item_target_path` 一致。**
    ///
    /// 两端各写各的会长成「同一条记录，桌面复制出来的路径与命令行打印的不是同一个」——
    /// 而两者都看起来完全正常，只有在用户拿命令行给的路径去桌面那边找不到时才显形。
    #[test]
    fn single_file_shows_itself_and_a_bundle_shows_its_root() {
        let file = json!({ "localPath": "/recv/solo.txt", "relativePath": "solo.txt" });
        let detail = json!({ "rootPath": "/recv" });

        assert_eq!(
            item_location(&detail, std::slice::from_ref(&file)),
            "/recv/solo.txt",
            "单文件条目取文件自身——用户接着要 cat / cp 它"
        );
        assert_eq!(
            item_location(&detail, &[file.clone(), file]),
            "/recv",
            "多文件条目取容器目录"
        );
    }

    /// 记录里没有位置时给占位符，**不是空字符串**——那会印出一行只有标签的输出，
    /// 看起来像程序算错了而不是「这条老记录没存」。
    #[test]
    fn a_record_without_a_location_shows_a_placeholder() {
        assert_eq!(item_location(&json!({}), &[]), "—");
    }

    /// **对端发来的控制字符不得原样进终端菜单。**
    ///
    /// 文本条目的标题就是对端的正文——一条含 `\x1b[2J` 的文本会在 dialoguer 重绘菜单时
    /// **清屏**，与这个函数要防的错位是同一类症状，只是更彻底。
    #[test]
    fn control_characters_never_reach_the_menu() {
        let line = title_line(&json!({ "title": "正常\u{1b}[2J\u{7}尾巴" }));
        assert!(
            !line.chars().any(char::is_control),
            "控制字符透出来了: {line:?}"
        );
        assert!(
            line.contains("正常") && line.contains("尾巴"),
            "把内容也滤没了"
        );
    }

    /// 多行标题压成一行，且只在真有换行时才动空白。
    ///
    /// 文件名里的连续空格是记录的真实内容，压掉它等于静默改写用户的文件名。
    #[test]
    fn titles_are_flattened_only_when_they_contain_line_breaks() {
        assert_eq!(
            title_line(&json!({ "title": "第一行\n\n第三行" })),
            "第一行 第三行"
        );
        assert_eq!(
            title_line(&json!({ "title": "my  file.txt" })),
            "my  file.txt"
        );
    }

    /// 超长标题截断并带省略号——菜单是按行绘制的，折行会让选中高亮错位。
    #[test]
    fn long_titles_are_truncated() {
        let long = "字".repeat(TITLE_MAX_CHARS + 10);
        let line = title_line(&json!({ "title": long }));
        assert_eq!(
            line.chars().count(),
            TITLE_MAX_CHARS + 1,
            "截断后多一个省略号"
        );
        assert!(line.ends_with('…'));

        let exact = "字".repeat(TITLE_MAX_CHARS);
        assert_eq!(
            title_line(&json!({ "title": exact.clone() })),
            exact,
            "正好压线不加省略号"
        );
    }
}
