//! `bootstrap`：引导 / 中继节点的清点与增删。
//!
//! **与 `config` 分成两个名词是刻意的。** 引导节点是一个**集合**，用户的意图是「加一条」
//! 「撤一条」；而 `config set` 表达的是整值替换。用后者表达集合，会迫使调用方先读出当前
//! 清单、编辑、再整份写回——那正是 `bootstrap-node-settings` 明令禁止的「持久化合并后的
//! 最终清单」，它在内置项更新时把老用户永久压在旧地址上。
//!
//! 三条动作全是 `Persisted` 档：清单就在本机的文件里，**改它不需要节点，也不为此拉起
//! 一个**。有常驻节点在跑时经它，于是增删当场生效、不必重启。

use crate::adapter::paths::DataDir;
use crate::cmd::BootstrapAction;
use crate::exit::{CliError, CliResult};
use crate::prompt::Question;
use crate::prompt::pick::Picker;
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::bootstrap_nodes::{BootstrapChanged, BootstrapRow};
use crate::runtime::ipc::Request;

pub async fn run(data_dir: &DataDir, json: bool, action: BootstrapAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        BootstrapAction::List => {
            crate::render::bootstrap::render_list(&list(&access).await?, json);
        }
        BootstrapAction::Add { addr } => add(&access, json, addr).await?,
        BootstrapAction::Remove { addr } => remove(&access, json, addr).await?,
    }

    Ok(())
}

async fn list(access: &RecordAccess) -> CliResult<Vec<BootstrapRow>> {
    let payload = access
        .query(Request::BootstrapList, |records| async move {
            to_value(
                &crate::runtime::bootstrap_nodes::list(&records, None).await?,
                "引导节点清单",
            )
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析引导节点清单: {err}")))
}

/// 添加：地址缺席时问一句（需要可交互的终端）。
///
/// 不弹菜单——没有候选集可列，用户要粘的是一条别处给他的地址。
async fn add(access: &RecordAccess, json: bool, addr: Option<String>) -> CliResult<()> {
    let addr = match addr {
        Some(addr) => addr,
        None => {
            Question::new("引导节点地址（multiaddr，需含 /p2p/<节点标识>）")
                .ask(
                    "请给出要添加的引导节点地址。\n\
                     当前环境无法交互输入；形如 /ip4/<地址>/tcp/<端口>/p2p/<节点标识>。",
                )
                .await?
        }
    };

    let changed = apply(
        access,
        Request::BootstrapAdd { addr: addr.clone() },
        move |records| {
            let addr = addr.clone();
            async move { crate::runtime::bootstrap_nodes::add(&records, None, &addr).await }
        },
    )
    .await?;

    crate::render::bootstrap::render_added(&changed, json);
    Ok(())
}

/// 撤销：地址缺席时列出清单让用户挑一条。
async fn remove(access: &RecordAccess, json: bool, addr: Option<String>) -> CliResult<()> {
    let chosen = Picker {
        fetch: async || list(access).await,
        label: crate::render::bootstrap::menu_line,
        prompt: "撤销哪一条引导节点？",
        empty: "生效清单里已经没有引导节点了",
        unavailable: "请指定要撤销的引导节点地址。\n\
                      当前环境无法交互选择；用 swarmdrop bootstrap list 查看有哪些。",
    }
    .one(addr.as_deref(), locate)
    .await?;

    // 传 canonical 地址而不是用户敲的那串：**前缀在「当前清单」里才谈得上唯一**，
    // 而那个清单客户端已经取过一次了。让服务端再解析一遍等于把同一段逻辑写两份。
    let addr = chosen.addr.clone();
    let changed = apply(
        access,
        Request::BootstrapRemove { addr: addr.clone() },
        move |records| {
            let addr = addr.clone();
            async move { crate::runtime::bootstrap_nodes::remove(&records, None, &addr).await }
        },
    )
    .await?;

    crate::render::bootstrap::render_removed(&changed, json);
    Ok(())
}

/// 增删共用的取数骨架：有常驻节点交给它，没有就本进程直接改。
///
/// `live: None` 那一半是本函数存在的理由——它必须在两条动作里都写成同一句，
/// 否则「无节点时不得为写入拉起一个」这条会在其中一条上被悄悄破坏。
async fn apply<F, Fut>(
    access: &RecordAccess,
    request: Request,
    local: F,
) -> CliResult<BootstrapChanged>
where
    F: FnOnce(crate::runtime::access::Records) -> Fut,
    Fut: std::future::Future<Output = CliResult<BootstrapChanged>>,
{
    let payload = access
        .query(request, |records| async move {
            to_value(&local(records).await?, "改动结果")
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析改动结果: {err}")))
}

/// 按用户敲的地址（可给唯一前缀）在清单里定位。
///
/// 允许前缀是因为 multiaddr 长到没法手敲，而用户在 `bootstrap list` 里看到的就是这一串。
fn locate(rows: &[BootstrapRow], target: &str) -> CliResult<BootstrapRow> {
    let target = target.trim();
    if let Some(exact) = rows.iter().find(|row| row.addr == target) {
        return Ok(exact.clone());
    }

    let mut prefixed = rows.iter().filter(|row| row.addr.starts_with(target));
    match (prefixed.next(), prefixed.next()) {
        (Some(only), None) => Ok(only.clone()),
        (Some(_), Some(_)) => Err(CliError::Usage(format!(
            "这个前缀对应清单里的多条引导节点: {target}\n多敲几位，或用完整地址。"
        ))),
        _ => Err(CliError::Usage(format!(
            "清单里没有这条引导节点: {target}\n用 swarmdrop bootstrap list 看有哪些。"
        ))),
    }
}
