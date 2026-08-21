//! `send`：向一台已配对设备发送文件、目录或一段文本。
//!
//! ## 两样东西共用一条命令
//!
//! 文件与文本走同一个动词，因为对用户是同一件事——**把这个给那台设备**。
//! `--text` 换的是被发送的东西，不是一个动作开关（判据见 [`super`] 的命令组织规则）。
//! 于是目标设备的补全、退出码分类、有没有常驻节点这三件事对两者只有一份实现。
//!
//! ## 三个参数缺席时的补法各不相同
//!
//! 目标设备有候选集（已配对设备表），所以走菜单；文件没有——候选集是整个文件系统，
//! 所以走带 Tab 补全的行输入；文本连候选集的概念都没有，所以按**正文从哪来**分流：
//! 管道读到 EOF，终端拉起 `$EDITOR`。
//!
//! ## 补参数**必须在起节点之前**
//!
//! 起一个临时节点要连引导节点、做 NAT 探测，以秒计；而用户可能在菜单里按 Esc 走人。
//! 顺序反过来的表现是：`swarmdrop send`（不带参数）在管道里会先花几秒起节点，
//! 然后才报「当前环境无法交互选择」——一个本可以立刻给出的用法错误。
//! 补参数只读本机记录（已配对设备表、文件系统、标准输入），不需要节点。
//!
//! 正文的**长度校验也在这一侧**，理由相同：64 KiB 上限撞上时，用户要的是立刻知道，
//! 而不是等一个临时节点起完、连上对端之后才被告知。

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::prompt::Question;
use crate::prompt::pick::Picker;
use crate::runtime::access::{NodeAccess, RecordAccess};
use crate::runtime::devices::{self, DeviceRow};
use crate::runtime::ipc::{Request, Response};
use crate::runtime::transfer::{ProgressOut, send_files, send_text};
use swarmdrop_core::transfer::text_delivery::MAX_TEXT_DELIVERY_BYTES;

pub async fn run(
    data_dir: &DataDir,
    json: bool,
    files: Vec<PathBuf>,
    to: Option<String>,
    text: Option<Option<String>>,
) -> CliResult<()> {
    let records = RecordAccess::open(data_dir).await;
    let target = choose_target(&records, to.as_deref()).await?;
    let payload = resolve_payload(files, text).await?;

    let access = NodeAccess::open(data_dir, json).await?;
    let result = deliver(&access, &payload, &target, json).await;
    access.close().await;
    result
}

/// 这次 `send` 要送的东西。
///
/// 做成一个枚举而不是「两个可选参数 + 到处判空」：从这里往下，每一处要么对两支都成立
/// （目标、通道、退出码），要么必须**穷尽地**为两支各写一行（发出、渲染）。
/// 判空的写法里，漏掉一支是静默的——文本那支会走进文件那条路径，报「没有可发送的文件」。
enum Payload {
    Files(Vec<PathBuf>),
    Text(String),
}

impl Payload {
    /// 交给常驻节点去做时的动词。
    ///
    /// **传节点标识而不是用户敲的那个名字**：名字可能重复，而这一侧已经把它解析成
    /// 唯一的一台了。让服务端拿着名字再解析一遍等于给了它一次解析成另一台的机会。
    fn request(&self, to: &str) -> Request {
        match self {
            Self::Files(files) => Request::Send {
                paths: files
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                to: to.to_owned(),
            },
            Self::Text(body) => Request::SendText {
                body: body.clone(),
                to: to.to_owned(),
            },
        }
    }

    /// 常驻节点做完之后，它给回来的已经是 JSON。
    fn render_remote(&self, data: &serde_json::Value, json: bool) {
        match self {
            Self::Files(_) => crate::render::send::render_from_json(data, json),
            Self::Text(_) => crate::render::send::render_text_from_json(data, json),
        }
    }
}

/// 有常驻节点就交给它，没有就用本进程刚起的这个临时节点自己做。
///
/// **「还在等」的转轮画在这一层**，因为这一层两条路径都经过——交给常驻节点做时，那边的
/// 输出进的是它自己的日志流，用户的终端上一个字都不会有。而文本这条等待可以长达五分钟
/// （对端策略要求人工确认时），一条五分钟不动也不报错的命令只会被读成卡死。
///
/// 文件那支**分两种情况**：本进程自持节点时不画，它有真正的进度条（[`send_files`] 自己
/// 画准备与传输两段），两个东西同时占着 stderr 会互相擦掉；而交给常驻节点做时那些事件
/// 都在**它那个进程**里，客户端这边一个字都不会有——那正是「输入完文件后卡住」的样子。
async fn deliver(
    access: &NodeAccess,
    payload: &Payload,
    target: &DeviceRow,
    json: bool,
) -> CliResult<()> {
    // **转轮的作用域到此为止，结果打印在它之后。** 让它活到函数末尾的话，indicatif 的
    // 定时重绘会盖在结果行上，收尾时的清行还会把结果一起擦掉——而这件事只在真终端里
    // 显形，管道与 CI 全绿。靠作用域保证，不靠记得在每个返回点手动收。
    let done = {
        // 文本只有一次阻塞调用，没有进度可言——转轮就是它能给的全部。
        let _waiting = match payload {
            Payload::Text(_) => Some(crate::render::send::Waiting::new(!json, &target.name)),
            Payload::Files(_) => None,
        };
        run_delivery(access, payload, target, json).await?
    };
    done.render(payload, json);
    Ok(())
}

/// 做完了，等着被渲染。
///
/// 存在的理由只有一个：**把「做」与「说」分成两段**，好让转轮的作用域夹在中间
/// （见 [`deliver`]）。
enum Delivered {
    /// 常驻节点做的，回来的已经是 JSON。
    Remote(serde_json::Value),
    Files(crate::runtime::transfer::SendOutcome),
    Text(crate::runtime::transfer::TextOutcome),
}

impl Delivered {
    fn render(&self, payload: &Payload, json: bool) {
        match self {
            Self::Remote(data) => payload.render_remote(data, json),
            Self::Files(outcome) => crate::render::send::render(outcome, json),
            Self::Text(outcome) => crate::render::send::render_text(outcome, json),
        }
    }
}

async fn run_delivery(
    access: &NodeAccess,
    payload: &Payload,
    target: &DeviceRow,
    json: bool,
) -> CliResult<Delivered> {
    let peer_id = &target.peer_id;

    // 文件交给常驻节点做时，进度由它推过来、由**这一侧**画（见
    // [`crate::render::send::RemoteProgress`]）。此前这条路径全程无输出，
    // 而那正是「输入完文件后卡住」的样子。
    //
    // 文本不需要：它是一次阻塞调用，没有中间进度，转轮已经在 [`deliver`] 里了。
    let mut remote = match payload {
        Payload::Files(_) => Some(crate::render::send::RemoteProgress::new(
            !json,
            &target.name,
        )),
        Payload::Text(_) => None,
    };

    let response = access
        .ask_watching(&payload.request(peer_id), |frame| {
            if let Some(remote) = remote.as_mut() {
                remote.on_frame(&frame);
            }
        })
        .await?;

    // **进度条必须在结果打印之前收干净**：indicatif 的定时重绘会盖在结果行上，
    // 收尾时的清行还会把结果一起擦掉。靠显式 drop 而不是等函数返回。
    drop(remote);

    match response {
        Some(Response::Data { payload: data }) => Ok(Delivered::Remote(data)),
        // 尊重服务端给的分类：它区分得出「对端不可达」与「传输中断」，
        // 而在这里一律按后者会让脚本的重试策略选错。
        Some(Response::Error { code, message }) => Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => {
            let node = access.require_local()?;
            match payload {
                // `!json`：结构化模式下 stdout 只能有最终结果，进度条一概不画。
                Payload::Files(files) => {
                    send_files(node, files, peer_id, ProgressOut::Bars { enabled: !json })
                        .await
                        .map(Delivered::Files)
                }
                Payload::Text(body) => send_text(node, body.clone(), peer_id)
                    .await
                    .map(Delivered::Text),
            }
        }
    }
}

/// 把参数补成一份确定的内容。**在起节点之前**（见模块文档）。
async fn resolve_payload(files: Vec<PathBuf>, text: Option<Option<String>>) -> CliResult<Payload> {
    let Some(inline) = text else {
        if files.is_empty() {
            return Ok(Payload::Files(ask_for_files().await?));
        }
        // 路径要在这里转成绝对：常驻节点可能在别的工作目录下启动，相对路径到了那边
        // 就指向别处。
        return files
            .iter()
            .map(|path| absolute(path))
            .collect::<CliResult<_>>()
            .map(Payload::Files);
    };
    ask_for_text(inline).await.map(Payload::Text)
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

/// 取到要发送的正文，并**在起节点之前**校验它。
///
/// 三条来源，判据是「用户把正文放哪儿了」：
///
/// | 写法 | 正文来自 |
/// |---|---|
/// | `--text <内容>` | 命令行本身 |
/// | `… \| swarmdrop send --text` | 标准输入，读到 EOF |
/// | `swarmdrop send --text`（终端） | `$EDITOR` |
///
/// **`-` 不是「读标准输入」的写法**（`kubectl -f -` 那个惯例）：管道那条已经由「只给
/// `--text`」表达了，再给一个等价写法只会让「发一条只有一个减号的消息」变得无法表达。
async fn ask_for_text(inline: Option<String>) -> CliResult<String> {
    const UNAVAILABLE: &str = "请给出要发送的文本。\n\
                               当前环境无法交互输入；形态是 swarmdrop send --text <内容> --to <设备>，\n\
                               或把正文从管道送进来。";

    let body = match inline {
        Some(body) => body,
        // 管道优先于编辑器：`… | swarmdrop send --text` 里没有人在看屏幕，
        // 这时拉起 `$EDITOR` 会把一个全屏程序接到一条管道上，两边都动不了。
        None if !std::io::stdin().is_terminal() => read_stdin().await?,
        // 空缓冲区 / 没保存就退出 / 编辑器起不来，对这里是同一件事：没有正文可发。
        None => crate::prompt::compose(UNAVAILABLE)
            .await?
            .ok_or(CliError::Aborted)?,
    };

    // 空与超限都在这里拦下，退出码是用法错误——它们要用户改输入，不是让脚本退避重试。
    //
    // **判定仍然由核心做**（`validate_text_body` 是那条规则唯一的权威，将来它加规则
    // 这里照样拦得住），这里只负责把已知的两种说人话：核心的错误 Display 带着
    // `Transfer error:` 前缀，那是给日志看的，不该出现在一条用法提示里。
    swarmdrop_core::transfer::text_delivery::validate_text_body(&body).map_err(|err| {
        CliError::Usage(match body.len() {
            0 => "文本内容不能为空".to_owned(),
            len if len > MAX_TEXT_DELIVERY_BYTES => too_long(),
            // 核心将来新增的规则从这里出来。带前缀但不失真，好过在这里猜它说了什么。
            _ => err.to_string(),
        })
    })?;
    Ok(body)
}

/// 超限的措辞。**两处共用一份**：这里与 [`read_stdin`] 各说各的话时，同一条规则会因为
/// 正文来自命令行还是管道而给出两种提示。
fn too_long() -> String {
    format!("文本内容不能超过 {MAX_TEXT_DELIVERY_BYTES} 字节")
}

/// 从标准输入最多读这么多字节。
///
/// 上限再加几个字节：多读出来的那几个才分得清「压线正文 + `\r\n`」与「真超了」。
/// 只多读一个的话，`\r\n` 结尾的压线正文会被误判——而它 trim 完是合法的。
/// 加 4 而不是 2：`take` 砍在多字节字符中间时 `from_utf8` 会失败，留一点余量让
/// 真正超限的输入走到「太长了」那条消息，而不是走到「不是合法 UTF-8」。
const READ_BUDGET: u64 = MAX_TEXT_DELIVERY_BYTES as u64 + 4;

/// 把标准输入读成正文。
///
/// **读进 `Vec<u8>` 再转 UTF-8，不直接 `read_to_string`**：那样两种失败会混成一种。
/// 超长的输入在 64 KiB 处截断时多半正好切在一个多字节字符中间，于是「文本太长」被报成
/// 「不是合法 UTF-8」——而用户只是 `cat` 错了一个文件。
///
/// 上限 + 1 是刻意的：多读那一个字节才分得清「正好 64 KiB」与「超了」，
/// 而截到上限再判等长会把恰好压线的合法输入误杀。
async fn read_stdin() -> CliResult<String> {
    // `spawn_blocking`：读管道会一直阻塞到对面写完，直接在异步运行时里读会占住一个 worker。
    let bytes = tokio::task::spawn_blocking(|| {
        let mut buf = Vec::new();
        std::io::stdin()
            .take(READ_BUDGET)
            .read_to_end(&mut buf)
            .map(|_| buf)
    })
    .await
    .map_err(|err| CliError::Usage(format!("读取标准输入失败: {err}")))?
    .map_err(|err| CliError::Usage(format!("读取标准输入失败: {err}")))?;

    // **先转文本、先削结尾换行，再判超限。** 顺序反过来的话，「正好 64 KiB 的正文 +
    // 一个结尾换行」（几乎每个编辑器保存时都会补那个换行）会在这里被拒，
    // 而用户去数一遍发现正好是上限、无从下手——本函数自己的文档还写着「结尾的换行
    // 几乎必然是噪声」，判定却把这个噪声算进了预算。
    let body = String::from_utf8(bytes)
        .map_err(|_| CliError::Usage("标准输入不是合法的 UTF-8 文本".into()))?;
    let body = trim_trailing_newlines(&body);
    if body.len() > MAX_TEXT_DELIVERY_BYTES {
        return Err(CliError::Usage(too_long()));
    }
    Ok(body.to_owned())
}

/// 去掉**结尾的换行**，其余空白一律保留。
///
/// 只动结尾：正文里的缩进是内容（发一段代码或配置时尤其），整体 trim 会把它吃掉。
/// 而结尾的换行几乎必然是噪声——`echo` 加的那个、编辑器保存时补的那个——同时这也让
/// 管道与 `$EDITOR` 两条路径的结果一致（dialoguer 的编辑器自带 `trim_newlines`），
/// 并与 shell 的 `$(…)` 惯例一致。
fn trim_trailing_newlines(body: &str) -> &str {
    body.trim_end_matches(['\n', '\r'])
}

/// 转成绝对路径，顺带校验它确实存在。
///
/// `canonicalize` 一次做两件事：解相对路径与解符号链接，而它对不存在的路径报错——
/// 于是「路径写错了」在这里就被拦下，不必等到起完节点、连上对端才发现。
fn absolute(path: &Path) -> CliResult<PathBuf> {
    std::fs::canonicalize(path)
        .map_err(|err| CliError::Usage(format!("无法解析路径 {}: {err}", path.display())))
}
#[cfg(test)]
mod tests {
    use super::*;

    /// **正文的边界在这一侧拦下，且是用法错误。**
    ///
    /// 放在这里而不是让核心去拦，是为了让「打错了」在**起节点之前**就退出：交给核心
    /// 意味着先花几秒连引导节点、做 NAT 探测，然后才被告知正文是空的。
    /// 退出码也必须是用法错误（2）而不是传输失败（5）——脚本对前者的处置是改输入，
    /// 对后者是退避重试，而重试一条空正文永远不会成功。
    #[tokio::test]
    async fn inline_text_is_validated_before_anything_else() {
        for body in ["", " \t "] {
            // 注意空白正文**不算空**（`validate_text_body` 只拒真空串），所以这里只有
            // 第一条会失败——第二条钉的正是「不许在这里偷偷 trim 掉用户的内容」。
            let result = ask_for_text(Some(body.to_owned())).await;
            if body.is_empty() {
                assert!(
                    matches!(result, Err(CliError::Usage(_))),
                    "空正文必须是用法错误"
                );
            } else {
                assert_eq!(result.expect("空白也是内容"), body);
            }
        }

        let at_limit = "a".repeat(MAX_TEXT_DELIVERY_BYTES);
        assert!(
            ask_for_text(Some(at_limit)).await.is_ok(),
            "正好压线的正文必须能发"
        );

        let over_limit = "a".repeat(MAX_TEXT_DELIVERY_BYTES + 1);
        assert!(
            matches!(
                ask_for_text(Some(over_limit)).await,
                Err(CliError::Usage(_))
            ),
            "超过上限必须是用法错误"
        );
    }

    /// 只削结尾的换行——缩进与内部空行都是内容。
    ///
    /// 削多了是**静默**的：用户发一段带缩进的代码，对端收到的是被拉平的版本，
    /// 两边都不会看到任何错误。
    #[test]
    fn only_trailing_newlines_are_trimmed() {
        assert_eq!(trim_trailing_newlines("hi\n"), "hi");
        assert_eq!(trim_trailing_newlines("hi\r\n"), "hi");
        assert_eq!(trim_trailing_newlines("hi\n\n\n"), "hi");
        assert_eq!(trim_trailing_newlines("  缩进"), "  缩进");
        assert_eq!(trim_trailing_newlines("上\n\n下\n"), "上\n\n下");
        // 结尾的空格不是换行，留着——它可能正是用户要发的东西（比如一段 diff）。
        assert_eq!(trim_trailing_newlines("hi \n"), "hi ");
    }
}
