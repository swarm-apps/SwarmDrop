//! `send`：向一台已配对设备发送文件或目录。
//!
//! ## 两个参数缺席时的补法不同
//!
//! 目标设备有候选集（已配对设备表），所以走菜单；文件没有——候选集是整个文件系统，
//! 所以走带 Tab 补全的行输入。
//!
//! ## 补参数**必须在起节点之前**
//!
//! 起一个临时节点要连引导节点、做 NAT 探测，以秒计；而用户可能在菜单里按 Esc 走人。
//! 顺序反过来的表现是：`swarmdrop send`（不带参数）在管道里会先花几秒起节点，
//! 然后才报「当前环境无法交互选择」——一个本可以立刻给出的用法错误。
//! 补参数只读本机记录（已配对设备表、文件系统），不需要节点。

use std::path::{Path, PathBuf};

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::prompt::Question;
use crate::prompt::pick::Picker;
use crate::runtime::access::{NodeAccess, RecordAccess};
use crate::runtime::devices::{self, DeviceRow};
use crate::runtime::ipc::{Request, Response};
use crate::runtime::transfer::send_files;

pub async fn run(
    data_dir: &DataDir,
    json: bool,
    files: Vec<PathBuf>,
    to: Option<String>,
) -> CliResult<()> {
    let records = RecordAccess::open(data_dir).await;
    let target = choose_target(&records, to.as_deref()).await?;

    let files = if files.is_empty() {
        ask_for_files().await?
    } else {
        // 路径要在这里转成绝对：常驻节点可能在别的工作目录下启动，相对路径到了那边
        // 就指向别处。
        files
            .iter()
            .map(|path| absolute(path))
            .collect::<CliResult<_>>()?
    };

    let access = NodeAccess::open(data_dir, json).await?;

    // **传节点标识而不是用户敲的那个名字**：名字可能重复，而这一侧已经把它解析成
    // 唯一的一台了。让服务端拿着名字再解析一遍等于给了它一次解析成另一台的机会。
    let request = Request::Send {
        paths: files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        to: target.peer_id.clone(),
    };

    let result = match access.ask(&request).await? {
        Some(Response::Data { payload }) => {
            crate::render::send::render_from_json(&payload, json);
            Ok(())
        }
        // 尊重服务端给的分类：它区分得出「对端不可达」与「传输中断」，
        // 而在这里一律按后者会让脚本的重试策略选错。
        Some(Response::Error { code, message }) => Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => {
            let node = access.require_local()?;
            match send_files(node, &files, &target.peer_id, !json).await {
                Ok(outcome) => {
                    crate::render::send::render(&outcome, json);
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
    };

    access.close().await;
    result
}

/// 定位目标设备：给了名称/标识就用它，没给就列出已配对设备让用户挑。
async fn choose_target(records: &RecordAccess, to: Option<&str>) -> CliResult<DeviceRow> {
    Picker {
        // 与 `device forget` 共用取数与定位——「哪台已配对设备」这件事只该有一份答案，
        // 两处各写一遍迟早在重名处理上分歧。
        fetch: async || devices::list(records).await,
        label: crate::render::device::menu_line,
        prompt: "发给哪台设备？",
        empty: "本机还没有已配对设备。先执行 swarmdrop invite create 配一台。",
        unavailable: "请用 --to 指定目标设备（名称或节点标识）。\n\
                      当前环境无法交互选择；用 swarmdrop device list 查看有哪些设备。",
    }
    .one(to, super::device::locate)
    .await
}

/// 逐行问要发什么，空行结束。
///
/// **一行可以给多个**：把几个文件一起拖进终端得到的就是一行——那一行由
/// [`paths::split_line`] 拆开。
///
/// 不存在的路径**只跳过那一个**，不让整轮作废：一次拖五个文件错一个，重来五次不合理。
async fn ask_for_files() -> CliResult<Vec<PathBuf>> {
    const UNAVAILABLE: &str = "请给出要发送的文件或目录。\n\
                               当前环境无法交互输入；形态是 swarmdrop send <路径>… --to <设备>。";

    let mut files: Vec<PathBuf> = Vec::new();
    loop {
        let prompt = if files.is_empty() {
            "要发送的文件或目录（可拖进终端 · Tab 补全）"
        } else {
            "还要加吗？（直接回车开始发送）"
        };

        let answered = Question::new(prompt)
            // 每条路径下面紧跟着会回显成 `+ …`，复述一遍等于把它写两次。
            .no_echo()
            .ask_paths(UNAVAILABLE)
            .await?;

        if answered.is_empty() {
            // 一个都没给就回车 ⇒ 用户改主意了。**不是「发送零个文件」**：
            // 那会一路走到起节点、连对端，最后才报「没有可发送的文件」。
            if files.is_empty() {
                return Err(CliError::Aborted);
            }
            return Ok(files);
        }

        for raw in answered {
            match absolute(&raw) {
                Ok(path) => {
                    // 逐个回显是为了让用户看清**拆行拆对了没有**——路径里的空格是这一步
                    // 最容易出错的地方。
                    crate::render::send::echo_added(&path);
                    files.push(path);
                }
                // 报错但不退出：接着问下一个，用户可以改正后重敲。
                Err(err) => crate::render::send::echo_rejected(&err),
            }
        }
    }
}

/// 转成绝对路径，顺带校验它确实存在。
///
/// `canonicalize` 一次做两件事：解相对路径与解符号链接，而它对不存在的路径报错——
/// 于是「路径写错了」在这里就被拦下，不必等到起完节点、连上对端才发现。
fn absolute(path: &Path) -> CliResult<PathBuf> {
    std::fs::canonicalize(path)
        .map_err(|err| CliError::Usage(format!("无法解析路径 {}: {err}", path.display())))
}
