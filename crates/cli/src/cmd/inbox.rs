//! `inbox`：收件箱的列出、查看与导出。
//!
//! `list` / `show` 是 `Persisted` 档；`export` 在取到详情之后还要复制文件，那一步纯本地。

use std::path::Path;

use serde_json::Value;

use crate::adapter::paths::DataDir;
use crate::cmd::InboxAction;
use crate::exit::{CliError, CliResult};
use crate::prompt::Question;
use crate::prompt::pick::Picker;
use crate::runtime::access::{RecordAccess, rows, to_value};
use crate::runtime::inbox as records_inbox;
use crate::runtime::ipc::Request;

pub async fn run(data_dir: &DataDir, json: bool, action: InboxAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        InboxAction::List => {
            crate::render::inbox::render_list(&list(&access).await?, json);
        }
        InboxAction::Show { id } => {
            let id = choose(&access, id, "查看哪个条目？").await?;
            crate::render::inbox::render_detail(&detail(&access, &id).await?, json);
        }
        InboxAction::Export { id, dir } => {
            let id = choose(&access, id, "导出哪个条目？").await?;
            let detail = detail(&access, &id).await?;

            // **目录在取到详情之后才问**：条目不存在时先问一个目录再报错，等于让用户
            // 白填一次。顺序反过来不会出错，只是多一次无用的提问。
            let dir = match dir {
                Some(dir) => dir,
                None => {
                    Question::new("导出到哪个目录？")
                        // 默认当前目录：`cp` 的形态，也是绝大多数场合的答案。
                        .default(".")
                        // 目录紧接着出现在「已导出 N 项到 …」里。
                        .no_echo()
                        // **`ask_path` 而非 `ask`**：补全写回去的是转义过的形式，
                        // 只 `ask` 会拿到一条还带着反斜杠的路径——含空格的目录名上才显形。
                        .ask_path(
                            "请给出导出目标目录。当前环境无法交互输入；\n\
                             形态是 swarmdrop inbox export <标识> <目录>。",
                        )
                        .await?
                }
            };

            let count = export(&detail, &id, &dir)?;
            crate::render::inbox::render_exported(count, &dir, json);
        }
    }

    Ok(())
}

/// 取一次收件箱清单（整段 JSON）。
async fn list(access: &RecordAccess) -> CliResult<Value> {
    access
        .query(Request::InboxList, |records| async move {
            let store = records.transfers().await?;
            to_value(&records_inbox::list(&*store).await?, "收件箱")
        })
        .await
}

/// 定位一个条目：给了标识就用它，没给就列出收件箱让用户挑。
///
/// **给了标识就不取清单**——标识是完整 UUID，自己就能定位，用不着先把整个收件箱拉回来。
/// 先在本地校验格式：格式错误是**用法错误**（改参数重来），与「没有这个条目」是两回事；
/// 通道那侧同样会校验，这里先做一次是为了在没有常驻节点时也立刻给出用法错误。
///
/// ⚠️ 与 `transfer show` 不同，这里**不能**把选中行直接当详情用：收件箱的清单是
/// `InboxItemSummary`、详情是 `InboxItemDetail`（多出文件条目与本地路径），是两个类型，
/// 所以选完还得再查一次。
///
/// `prompt` 由调用方给——「查看哪个」与「导出哪个」是两件事，菜单该说清楚是哪一件。
async fn choose(access: &RecordAccess, id: Option<String>, prompt: &str) -> CliResult<String> {
    if let Some(id) = id {
        records_inbox::parse_id(&id)?;
        return Ok(id);
    }

    let chosen = Picker {
        fetch: async || Ok(rows(list(access).await?)),
        label: crate::render::inbox::menu_line,
        prompt,
        empty: "收件箱是空的",
        unavailable: "请指定条目标识。\n\
                      当前环境无法交互选择；用 swarmdrop inbox list 查看有哪些。",
    }
    .menu()
    .await?;

    // 菜单那条路径上的行直接来自收件箱清单，`id` 必在。
    Ok(crate::render::text_or(&chosen, "id", ""))
}

/// 取一个条目的详情。
async fn detail(access: &RecordAccess, id: &str) -> CliResult<Value> {
    let owned = id.to_owned();
    access
        .query(
            Request::InboxShow { id: id.to_owned() },
            move |records| async move {
                let store = records.transfers().await?;
                to_value(&records_inbox::detail(&*store, &owned).await?, "条目详情")
            },
        )
        .await
}

/// 导出：把条目的文件复制到目标目录。
///
/// **复制而非移动**：收件箱记录仍指向原位置，移动会让那条记录变成失效链接。
/// 在客户端做而不是让服务端做——目标目录是用户在**这个** shell 里给的路径。
fn export(detail: &Value, id: &str, to: &Path) -> CliResult<usize> {
    std::fs::create_dir_all(to)
        .map_err(|err| CliError::Usage(format!("创建目标目录失败: {err}")))?;

    // 文本条目：正文直接落一个文件。
    if detail.pointer("/content/kind").and_then(Value::as_str) == Some("text") {
        let body = detail
            .pointer("/content/body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        std::fs::write(to.join(format!("{id}.txt")), body)
            .map_err(|err| CliError::Usage(format!("写入文本失败: {err}")))?;
        return Ok(1);
    }

    let entries = detail
        .pointer("/content/entries")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::Usage("条目内容无法解析".into()))?;

    let mut exported = 0usize;
    for entry in entries {
        if crate::render::flag(entry, "missing") {
            eprintln!(
                "跳过已丢失的文件: {}",
                entry
                    .get("relativePath")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
            );
            continue;
        }
        let (Some(relative), Some(local)) = (
            entry.get("relativePath").and_then(Value::as_str),
            entry.get("localPath").and_then(Value::as_str),
        ) else {
            continue;
        };

        let target = to.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| CliError::Usage(format!("创建目录失败: {err}")))?;
        }
        std::fs::copy(local, &target)
            .map_err(|err| CliError::Usage(format!("复制 {relative} 失败: {err}")))?;
        exported += 1;
    }
    Ok(exported)
}
