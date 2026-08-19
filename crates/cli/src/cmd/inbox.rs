//! `inbox`：收件箱的列出、查看与导出。
//!
//! `list` / `show` 是 `Persisted` 档；`export` 在取到详情之后还要复制文件，那一步纯本地。

use std::path::Path;

use serde_json::Value;

use crate::adapter::paths::DataDir;
use crate::cmd::InboxAction;
use crate::exit::{CliError, CliResult};
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::inbox as records_inbox;
use crate::runtime::ipc::Request;

pub async fn run(data_dir: &DataDir, json: bool, action: InboxAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        InboxAction::List => {
            let items = access
                .query(Request::InboxList, |records| async move {
                    let store = records.transfers().await?;
                    to_value(&records_inbox::list(&*store).await?, "收件箱")
                })
                .await?;
            crate::render::inbox::render_list(&items, json);
        }
        InboxAction::Show { id } => {
            let detail = detail(&access, &id).await?;
            crate::render::inbox::render_detail(&detail, json);
        }
        InboxAction::Export { id, dir } => {
            let detail = detail(&access, &id).await?;
            let count = export(&detail, &id, &dir)?;
            crate::render::inbox::render_exported(count, &dir, json);
        }
    }

    Ok(())
}

/// 取一个条目的详情。
async fn detail(access: &RecordAccess, id: &str) -> CliResult<Value> {
    // 先在本地校验格式：格式错误是**用法错误**，与「没有这个条目」是两回事。
    // 通道那侧同样会校验，这里先做一次是为了在没有常驻节点时也立刻给出用法错误。
    records_inbox::parse_id(id)?;

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
        if entry.get("missing").and_then(Value::as_bool) == Some(true) {
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
