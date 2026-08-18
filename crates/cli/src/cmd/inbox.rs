//! `inbox`：收件箱的列出、查看与导出。
//!
//! **有常驻节点就走本地通道，没有才直连数据库。**
//!
//! 这不是优化，是正确性：`migration` 的连接不设 `journal_mode`，走 SQLite 的 `delete`
//! 模式——那模式下写事务会阻塞所有读，而常驻节点接收文件时一直在写。直连会撞上
//! `database is locked`。反过来，没有常驻节点时也不该为了看一眼收件箱就起一个 P2P 节点
//! （要连引导节点，慢且没必要），那时也没有并发写者，直连是安全的。

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use swarmdrop_core::transfer::store::TransferStore;
use uuid::Uuid;

use crate::adapter::paths::DataDir;
use crate::cmd::InboxAction;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{self, Request, Response};

pub async fn run(data_dir: &DataDir, json: bool, action: InboxAction) -> CliResult<()> {
    let via_ipc = ipc::is_alive(&data_dir.socket()).await;

    match action {
        InboxAction::List => {
            let items = fetch(data_dir, via_ipc, Request::InboxList, Query::List).await?;
            crate::render::inbox::render_list(&items, json);
        }
        InboxAction::Get { id } => {
            let detail = fetch(
                data_dir,
                via_ipc,
                Request::InboxGet { id: id.clone() },
                Query::Detail(id),
            )
            .await?;
            crate::render::inbox::render_detail(&detail, json);
        }
        InboxAction::Export { id, to } => {
            let detail = fetch(
                data_dir,
                via_ipc,
                Request::InboxGet { id: id.clone() },
                Query::Detail(id.clone()),
            )
            .await?;
            let count = export(&detail, &id, &to)?;
            crate::render::inbox::render_exported(count, &to, json);
        }
    }
    Ok(())
}

/// 直连时要执行哪种查询。
enum Query {
    List,
    Detail(String),
}

/// 取数据：优先走通道，否则直连库。两条路都产出同一形状的 JSON。
async fn fetch(
    data_dir: &DataDir,
    via_ipc: bool,
    request: Request,
    query: Query,
) -> CliResult<Value> {
    if via_ipc {
        return match ipc::request(&data_dir.socket(), &request).await? {
            Some(Response::Data { payload }) => Ok(payload),
            Some(Response::Error { message }) => Err(CliError::NodeUnavailable(message)),
            // 通道刚才还活着、现在没了：节点在这一瞬关停了。直连兜底而不是报错——
            // 此刻已经没有并发写者，正是直连安全的时候。
            Some(Response::Ok) | None => direct(data_dir, query).await,
        };
    }
    direct(data_dir, query).await
}

async fn direct(data_dir: &DataDir, query: Query) -> CliResult<Value> {
    let db = migration::connect_and_migrate(&data_dir.database())
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("打开数据库失败: {err}")))?;
    let store: Arc<dyn TransferStore> =
        Arc::new(swarmdrop_storage_sql::SqlSessionStore::new(Arc::new(db)));

    match query {
        Query::List => {
            let items = store
                .list_inbox_items(false)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("读取收件箱失败: {err}")))?;
            serde_json::to_value(items)
                .map_err(|err| CliError::NodeUnavailable(format!("序列化失败: {err}")))
        }
        Query::Detail(id) => {
            let detail = store
                .get_inbox_item_detail(parse_id(&id)?)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("读取条目失败: {err}")))?
                .ok_or_else(|| CliError::Usage(format!("收件箱里没有条目 {id}")))?;
            serde_json::to_value(detail)
                .map_err(|err| CliError::NodeUnavailable(format!("序列化失败: {err}")))
        }
    }
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

fn parse_id(id: &str) -> CliResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| CliError::Usage(format!("不是合法的条目标识: {id}")))
}
