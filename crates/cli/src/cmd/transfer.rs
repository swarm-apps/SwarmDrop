//! `transfer`：传输记录的清点、查看，以及正在进行的那几条的实时监视与控制。
//!
//! 三条命令三个档（判据见 [`crate::runtime::access`]）：
//!
//! | 命令 | 档 | 为什么 |
//! |---|---|---|
//! | `list` / `show` | `RecordAccess` | 记录全在本机的库里，读它不需要节点 |
//! | `watch` | `RecordAccess` | 同上；没有常驻节点时它照样有用——会列出等着续传的那几条 |
//! | `pause` / `resume` / `cancel` | `DaemonAccess` | 作用于常驻节点内存里的活 actor，**临时节点里没有它们** |

use std::time::Duration;

use serde_json::Value;

use crate::adapter::paths::DataDir;
use crate::cmd::TransferAction;
use crate::exit::{CliError, CliResult};
use crate::prompt::pick::Picker;
use crate::prompt::{self, Hotkey};
use crate::render::watch::Panel;
use crate::runtime::access::{DaemonAccess, RecordAccess, rows, to_value};
use crate::runtime::ipc::Request;
use crate::runtime::transfers::{self, Control};

/// 面板重取一次事实的间隔。
///
/// **不是靠事件推送**：进度发生在常驻节点进程里，而本地通道是一问一答的（理由见
/// [`crate::runtime::ipc`]）——把它升级成流式会话，要连带引入超时、心跳与半关闭语义，
/// 那是把一条内部调试通道变成一套协议。一秒一次的代价是一次本地套接字往返加一条
/// 窄查询（`list_unfinished_projections`，只读没传完的那几条），而人眼分辨不出
/// 一秒的滞后。
const REFRESH: Duration = Duration::from_secs(1);

pub async fn run(data_dir: &DataDir, json: bool, action: TransferAction) -> CliResult<()> {
    match action {
        TransferAction::List => {
            let access = RecordAccess::open(data_dir).await;
            crate::render::transfer::render_list(&list(&access).await?, json);
        }
        // **两条路径各自取一次数，谁也不多取。** 给了标识就直查那一条（不必为了查一条
        // 记录先把几百条列出来）；没给则菜单里选中的那一行**本身就是详情**——清单与详情
        // 是同一个 `TransferProjection`，再查一次拿回的是同一份数据。
        TransferAction::Show { id } => {
            let access = RecordAccess::open(data_dir).await;
            let record = match id {
                Some(id) => show(&access, id).await?,
                None => picker(&access).menu().await?,
            };
            crate::render::transfer::render_detail(&record, json);
        }
        TransferAction::Watch => watch(data_dir, json).await?,
        TransferAction::Pause { ids } => control(data_dir, json, Control::Pause, &ids).await?,
        TransferAction::Resume { ids } => control(data_dir, json, Control::Resume, &ids).await?,
        TransferAction::Cancel { ids } => control(data_dir, json, Control::Cancel, &ids).await?,
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

/// 取一次**未完成**的传输（面板与三个动作的候选集都来自它）。
async fn unfinished(access: &RecordAccess) -> CliResult<Vec<Value>> {
    let payload = access
        .query(Request::TransferUnfinished, |records| async move {
            let store = records.transfers().await?;
            to_value(&transfers::unfinished(&*store).await?, "传输记录")
        })
        .await?;
    Ok(rows(payload))
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

// ============ 暂停 / 恢复 / 取消 ============

/// 执行一次运行控制。
///
/// 候选集**按动作过滤**（[`Control::applies`]），所以菜单里列出来的每一条都当真做得成，
/// 而参数指定的那条若不在其中会得到一句用法错误——不是等到服务端才从域深处报出来。
async fn control(data_dir: &DataDir, json: bool, action: Control, ids: &[String]) -> CliResult<()> {
    let daemon = DaemonAccess::open(data_dir).await?;

    let picked = control_picker(&daemon, action).many(ids, locate).await?;
    // 去重理由同 `invite revoke` / `device forget`：同一条会话可以被列两次（参数里写重了），
    // 不去重的后果是**虚报**——「已暂停 2 条」而其实只有一条。
    let picked = super::dedup_by_id(picked, |row| session_id(row).to_owned());

    let outcome = apply(&daemon, action, &picked).await?;
    crate::render::transfer::render_control(action, &outcome, json);
    Ok(())
}

/// 把选中的那几条送去执行。
///
/// 两个调用点（命令与面板热键）共用：拆开写的话，「哪个字段是标识」这件事会有两份答案。
async fn apply(daemon: &DaemonAccess, action: Control, picked: &[Value]) -> CliResult<Value> {
    daemon
        .ask(Request::TransferControl {
            action,
            ids: picked
                .iter()
                .map(|row| session_id(row).to_owned())
                .collect(),
        })
        .await
}

/// 候选集**来自常驻节点**，不是库里的一份快照。
///
/// 这不只是省一次通道探测：动作作用的是那个节点内存里的活 actor，所以「能不能做」
/// 必须按它此刻的状态判断。经 [`RecordAccess`] 取的话，节点恰好在这一瞬关停时会
/// 静默回落到直连数据库——于是菜单里列出一条「正在传输」，而它其实已经没了。
fn control_picker<'a>(
    daemon: &'a DaemonAccess,
    action: Control,
) -> Picker<'a, impl AsyncFn() -> CliResult<Vec<Value>> + 'a, impl Fn(&Value) -> String> {
    let (prompt, empty) = match action {
        Control::Pause => ("暂停哪几条传输？", "现在没有正在传输的会话"),
        Control::Resume => ("恢复哪几条传输？", "没有可恢复的传输"),
        Control::Cancel => ("取消哪几条传输？", "没有尚未结束的传输"),
    };
    Picker {
        fetch: move || async move {
            let candidates = rows(daemon.ask(Request::TransferUnfinished).await?);
            Ok(candidates
                .into_iter()
                .filter(|row| action.applies(row))
                .collect())
        },
        label: crate::render::transfer::menu_line,
        prompt,
        empty,
        unavailable: "请指定要操作的会话标识。\n\
                      当前环境无法交互选择；用 swarmdrop transfer list 查看有哪些。",
    }
}

/// 在候选集里按**完整**会话标识定位。
///
/// 不支持前缀：会话标识是 UUID，用户不会手敲它——主路径是勾选菜单，参数路径服务的是
/// 脚本（那里的标识来自 `transfer list --json`，天然是完整的）。为一条没人走的路
/// 再养一套前缀解析，只会多一处会与邀请那套漂移的规则。
fn locate(rows: &[Value], arg: &str) -> CliResult<Value> {
    // 格式错误先报出来：它要用户改参数，而「不在候选集里」要用户换一条。
    transfers::parse_id(arg)?;
    rows.iter()
        .find(|row| session_id(row) == arg)
        .cloned()
        .ok_or_else(|| {
            CliError::Usage(format!(
                "{arg} 不在可操作的传输里——它可能已经结束，或当前状态不支持这个动作。\n\
                 用 swarmdrop transfer list 看它现在是什么状态。"
            ))
        })
}

fn session_id(row: &Value) -> &str {
    row.get("sessionId").and_then(Value::as_str).unwrap_or("")
}

// ============ 实时面板 ============

/// 盯着正在进行的传输，直到用户停下。
///
/// 结构化模式下退化成 **NDJSON 快照流**（每轮一行）而不是画面板：那个模式声明的是
/// 「调用方是程序」，而程序读不了进度条。
async fn watch(data_dir: &DataDir, json: bool) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;
    if json {
        return stream_snapshots(&access).await;
    }

    // 热键要读 stdin，而这条判据（有没有终端 / 用户禁没禁交互）必须只回答一次。
    // 问不了人时面板照画——**只看不能操作**仍然有用（另开一个终端跑 `transfer pause`）。
    let hotkeys = prompt::can_ask();
    let mut panel = Panel::new(hotkeys);

    let mut ticker = tokio::time::interval(REFRESH);
    // 一次都不追赶：面板被热键菜单挡住几十秒后，`Burst` 会把攒下的 tick 一次性放出来，
    // 表现是回到面板的瞬间连刷十几屏。
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // 至多一个线程在读 stdin：热键读取一次一个，读到之后才再开一个。这既避免了
    // 多个读者抢同一份输入，也保证**弹菜单期间没有任何线程在读**——否则菜单的
    // 方向键会被面板的读者截走，用户看到的是一个动不了的选择框。
    let mut pending = hotkeys.then(|| Box::pin(prompt::hotkey()));

    loop {
        tokio::select! {
            // 先看键：用户按了 q 却因为这一轮取数失败而看到一句报错，是最没道理的。
            biased;

            key = async { pending.as_mut().expect("已判定为 Some").await }, if pending.is_some() => {
                match key {
                    Hotkey::Interrupt => return Ok(()),
                    Hotkey::Char('q') => return Ok(()),
                    Hotkey::Char(c) => {
                        if let Some(action) = action_for(c) {
                            // 腾出屏幕再弹菜单：进度条与选择框抢同一片区域，
                            // 叠在一起时两边都看不清。面板在菜单之后重建。
                            drop(panel);
                            let note = run_hotkey(data_dir, action).await;
                            panel = Panel::new(hotkeys);
                            if let Some(line) = note {
                                panel.note(&line);
                            }
                        }
                    }
                    Hotkey::Other => {}
                }
                // 上一个读者拿到键之后就结束了，**现在才开下一个**——期间弹过的菜单
                // 因此是 stdin 唯一的读者。同时这也换掉了已完成的那个 future：
                // `select!` 的条件下一轮仍会为真，留着它就是重复 poll 一个已完成的
                // future。
                pending = Some(Box::pin(prompt::hotkey()));
            }

            // 读键线程持有终端时 Ctrl-C 是**一个键**（raw 模式关掉了 `ISIG`），走上面
            // 那条路；这一条接的是没有读键线程的情形——`--no-input` 下的面板，
            // 以及两次读键之间那个极短的窗口。
            //
            // **必须显式接住而不是让默认信号处理器杀掉进程**：那样面板不会收尾，
            // 半屏进度条会留在用户的 shell 提示符上面。
            _ = tokio::signal::ctrl_c() => return Ok(()),

            _ = ticker.tick() => match unfinished(&access).await {
                Ok(records) => panel.sync(&records),
                // **取数失败不退出**：最常见的原因是常驻节点刚刚关停，而那时用户正看着
                // 一屏进度条——直接退出会让他以为是自己按错了键。留在面板上说一句，
                // 下一轮自然会恢复（或者他自己按 q 走）。
                Err(err) => panel.note(&format!("! {err}")),
            },
        }
    }
}

/// 结构化模式：每轮一行 JSON 快照，直到被中断。
///
/// 与人类模式共用同一份取数，所以两者看到的永远是同一组会话。
///
/// **取数失败在这里是退出、在面板上是留一行话**，这个不对称是刻意的：面板前面坐着一个
/// 人，他看得懂「节点刚停了」并且可能正等着它回来；而这边的消费者是程序，它需要的是一个
/// 非零退出码去决定重试还是放弃，继续吐空快照只会让它以为一切正常。
async fn stream_snapshots(access: &RecordAccess) -> CliResult<()> {
    let mut ticker = tokio::time::interval(REFRESH);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            // 这个模式下没有热键可按，Ctrl-C 是唯一的出口——而它必须是**正常退出**，
            // 否则消费方（脚本）会把一次正常的结束读成失败。
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = ticker.tick() => {
                let records = unfinished(access).await?;
                println!("{}", Value::Array(records));
            }
        }
    }
}

/// 热键到动作的映射。
///
/// 首字母取自动作的英文名（pause / resume / cancel），与三条子命令同名——
/// 面板上的热键与命令行是同一件事，记住一个就够。
fn action_for(key: char) -> Option<Control> {
    match key {
        'p' => Some(Control::Pause),
        'r' => Some(Control::Resume),
        'c' => Some(Control::Cancel),
        _ => None,
    }
}

/// 面板里按下热键之后：弹菜单、执行、把结果凝成一行话。
///
/// **返回一句话而不是直接打印**：面板还没重建，此刻打印会被随后的重绘覆盖掉。
/// 也因此它不复用 [`crate::render::transfer::render_control`]——那份是给命令的最终
/// 输出用的（多行、走 stdout），而这里要的是能塞进面板上方的一行。
async fn run_hotkey(data_dir: &DataDir, action: Control) -> Option<String> {
    match hotkey_control(data_dir, action).await {
        Ok(line) => Some(line),
        // 中止（Esc / 空勾选）是用户看过之后决定不动手，不该报成错误。
        Err(CliError::Aborted) => None,
        Err(err) => Some(format!("! {err}")),
    }
}

async fn hotkey_control(data_dir: &DataDir, action: Control) -> CliResult<String> {
    // **每次按键都重新探一次**：面板可能已经开着好几分钟，而常驻节点在这期间被停掉，
    // 此时该说的是「节点没在跑」，而不是从域深处报一个会话不存在。
    let daemon = DaemonAccess::open(data_dir).await?;
    // 走 `many(&[], ..)` 而不是一个公开的 `menu_many`：空参数正是「列出来让我挑」，
    // 三态骨架里已经有这条路（见 `prompt::pick`），另开一个入口只会多一处可绕过
    // 「问不了人就立刻退出」的口子。
    let picked = control_picker(&daemon, action).many(&[], locate).await?;
    let outcome = apply(&daemon, action, &picked).await?;
    Ok(crate::render::transfer::control_summary(action, &outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 热键与三条子命令同名同首字母——面板上的操作与命令行必须是同一件事，
    /// 否则用户得记两套。
    #[test]
    fn hotkeys_match_the_subcommands() {
        assert_eq!(action_for('p'), Some(Control::Pause));
        assert_eq!(action_for('r'), Some(Control::Resume));
        assert_eq!(action_for('c'), Some(Control::Cancel));
        // 未映射的键什么都不做——尤其不能落进某个默认动作。
        assert_eq!(action_for('x'), None);
        assert_eq!(action_for('\n'), None);
    }

    /// 参数里给的标识必须在**按动作过滤后**的候选集里，不在就是用法错误。
    ///
    /// 这条看守的是「用户对一条已经传完的会话敲 pause」：不拦的话，那串标识会一路
    /// 送到服务端，从域深处报一个「发送会话不存在」——而真正的原因是它已经完成了。
    #[test]
    fn an_id_outside_the_candidates_is_a_usage_error() {
        let id = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let rows = vec![json!({ "sessionId": id })];

        assert_eq!(session_id(&locate(&rows, &id).expect("应当定位到")), id);

        let err = locate(&rows, &other).expect_err("不在候选集里应当报错");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    /// 标识格式不合法要单独报——它要用户改参数，而「不在候选集里」要用户换一条。
    #[test]
    fn a_malformed_id_is_reported_before_the_lookup() {
        let err = locate(&[], "not-a-uuid").expect_err("应当拒绝");
        assert_eq!(err.code(), crate::exit::Code::Usage);
        assert!(err.to_string().contains("会话标识"), "{err}");
    }

    /// 形状不对的行不得让命令 panic——它可能来自一个更新过的常驻节点。
    #[test]
    fn a_row_without_an_id_yields_an_empty_string() {
        assert_eq!(session_id(&json!({})), "");
    }
}
