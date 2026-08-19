//! `transfer`：传输记录的清点与查看。
//!
//! 两条都是 `Persisted` 档——记录全在本机的库里，**读它不需要节点**。

use crate::adapter::paths::DataDir;
use crate::cmd::TransferAction;
use crate::exit::CliResult;
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::ipc::Request;
use crate::runtime::transfers;

pub async fn run(data_dir: &DataDir, json: bool, action: TransferAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        TransferAction::List => {
            let payload = access
                .query(Request::TransferList, |records| async move {
                    let store = records.transfers().await?;
                    to_value(&transfers::list(&*store).await?, "传输记录")
                })
                .await?;
            crate::render::transfer::render_list(&payload, json);
        }
        TransferAction::Show { id } => {
            // 先在本地校验格式：格式错误是**用法错误**（改参数重来），而「没有这条记录」
            // 是另一回事。不先校验的话两者会一起落进后者。
            transfers::parse_id(&id)?;
            let payload = access
                .query(Request::TransferShow { id: id.clone() }, |records| {
                    let id = id.clone();
                    async move {
                        let store = records.transfers().await?;
                        to_value(&transfers::show(&*store, &id).await?, "传输记录")
                    }
                })
                .await?;
            crate::render::transfer::render_detail(&payload, json);
        }
    }

    Ok(())
}
