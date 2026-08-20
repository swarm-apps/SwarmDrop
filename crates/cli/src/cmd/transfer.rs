//! `transfer`：传输记录的清点与查看。
//!
//! 两条都是 `Persisted` 档——记录全在本机的库里，**读它不需要节点**。

use serde_json::Value;

use crate::adapter::paths::DataDir;
use crate::cmd::TransferAction;
use crate::exit::CliResult;
use crate::prompt::pick::Picker;
use crate::runtime::access::{RecordAccess, rows, to_value};
use crate::runtime::ipc::Request;
use crate::runtime::transfers;

pub async fn run(data_dir: &DataDir, json: bool, action: TransferAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        TransferAction::List => {
            crate::render::transfer::render_list(&list(&access).await?, json);
        }
        // **两条路径各自取一次数，谁也不多取。** 给了标识就直查那一条（不必为了查一条
        // 记录先把几百条列出来）；没给则菜单里选中的那一行**本身就是详情**——清单与详情
        // 是同一个 `TransferProjection`，再查一次拿回的是同一份数据。
        TransferAction::Show { id } => {
            let record = match id {
                Some(id) => show(&access, id).await?,
                None => picker(&access).menu().await?,
            };
            crate::render::transfer::render_detail(&record, json);
        }
    }

    Ok(())
}

/// 取一次传输记录清单（整段 JSON）。
async fn list(access: &RecordAccess) -> CliResult<Value> {
    access
        .query(Request::TransferList, |records| async move {
            let store = records.transfers().await?;
            to_value(&transfers::list(&*store).await?, "传输记录")
        })
        .await
}

/// 按会话标识取一条。
///
/// 先在本地校验格式：格式错误是**用法错误**（改参数重来），而「没有这条记录」是另一回事。
/// 不先校验的话两者会一起落进后者。
async fn show(access: &RecordAccess, id: String) -> CliResult<Value> {
    transfers::parse_id(&id)?;
    access
        .query(
            Request::TransferShow { id: id.clone() },
            |records| async move {
                let store = records.transfers().await?;
                to_value(&transfers::show(&*store, &id).await?, "传输记录")
            },
        )
        .await
}

fn picker(
    access: &RecordAccess,
) -> Picker<'_, impl AsyncFn() -> CliResult<Vec<Value>>, impl Fn(&Value) -> String> {
    Picker {
        fetch: async || Ok(rows(list(access).await?)),
        label: crate::render::transfer::menu_line,
        prompt: "查看哪条传输记录？",
        empty: "还没有传输记录",
        unavailable: "请指定要查看的会话标识。\n\
                      当前环境无法交互选择；用 swarmdrop transfer list 查看有哪些。",
    }
}
