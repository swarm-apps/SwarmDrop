//! `inbox`：收件箱的列出、查看与导出。
//!
//! **这三条都不启动节点**：收件箱是本地数据，读它不需要联网。起一个 P2P 节点只为列个
//! 清单，既慢（要连引导节点）又会与常驻节点争用单实例锁。

use std::path::PathBuf;
use std::sync::Arc;

use swarmdrop_core::transfer::store::TransferStore;
use uuid::Uuid;

use crate::adapter::paths::DataDir;
use crate::cmd::InboxAction;
use crate::exit::{CliError, CliResult};

pub async fn run(data_dir: &DataDir, json: bool, action: InboxAction) -> CliResult<()> {
    let store = open_store(data_dir).await?;

    match action {
        InboxAction::List => list(&*store, json).await,
        InboxAction::Get { id } => get(&*store, json, &id).await,
        InboxAction::Export { id, to } => export(&*store, json, &id, &to).await,
    }
}

/// 只连数据库，不起节点。
///
/// 与常驻节点并存是安全的：SQLite 允许多个进程同时读，而这三条命令都不写。
async fn open_store(data_dir: &DataDir) -> CliResult<Arc<dyn TransferStore>> {
    let db = migration::connect_and_migrate(&data_dir.database())
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("打开数据库失败: {err}")))?;
    Ok(Arc::new(swarmdrop_storage_sql::SqlSessionStore::new(
        Arc::new(db),
    )))
}

async fn list(store: &dyn TransferStore, json: bool) -> CliResult<()> {
    let items = store
        .list_inbox_items(false)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取收件箱失败: {err}")))?;
    crate::render::inbox::render_list(&items, json);
    Ok(())
}

async fn get(store: &dyn TransferStore, json: bool, id: &str) -> CliResult<()> {
    let detail = store
        .get_inbox_item_detail(parse_id(id)?)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取条目失败: {err}")))?
        .ok_or_else(|| CliError::Usage(format!("收件箱里没有条目 {id}")))?;
    crate::render::inbox::render_detail(&detail, json);
    Ok(())
}

/// 导出：把条目的文件复制到目标目录。
///
/// **复制而非移动**：收件箱记录仍指向原位置，移动会让那条记录变成失效链接。
async fn export(store: &dyn TransferStore, json: bool, id: &str, to: &PathBuf) -> CliResult<()> {
    use swarmdrop_core::transfer::inbox::InboxItemContent;

    let detail = store
        .get_inbox_item_detail(parse_id(id)?)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取条目失败: {err}")))?
        .ok_or_else(|| CliError::Usage(format!("收件箱里没有条目 {id}")))?;

    std::fs::create_dir_all(to)
        .map_err(|err| CliError::Usage(format!("创建目标目录失败: {err}")))?;

    let mut exported = 0usize;
    match detail.content {
        InboxItemContent::Files { entries, .. } => {
            for entry in entries {
                if entry.missing {
                    eprintln!("跳过已丢失的文件: {}", entry.relative_path);
                    continue;
                }
                let target = to.join(&entry.relative_path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|err| CliError::Usage(format!("创建目录失败: {err}")))?;
                }
                std::fs::copy(&entry.local_path, &target).map_err(|err| {
                    CliError::Usage(format!("复制 {} 失败: {err}", entry.relative_path))
                })?;
                exported += 1;
            }
        }
        InboxItemContent::Text { body } => {
            let target = to.join(format!("{id}.txt"));
            std::fs::write(&target, body)
                .map_err(|err| CliError::Usage(format!("写入文本失败: {err}")))?;
            exported = 1;
        }
    }

    crate::render::inbox::render_exported(exported, to, json);
    Ok(())
}

fn parse_id(id: &str) -> CliResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| CliError::Usage(format!("不是合法的条目标识: {id}")))
}
