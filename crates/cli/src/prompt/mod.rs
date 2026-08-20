//! 向用户提问。
//!
//! 与 [`crate::render`] 分开是刻意的：那边是单向输出，这边要读 stdin，因此依赖
//! 「此刻有没有终端」这个环境事实。混进 render 会让这个能力判断散进每一处渲染，
//! 而它恰恰是**必须集中回答一次**的问题——命令行宿主同样跑在服务单元、cron 与
//! agent harness 里，那些场景没有人可问，问了就是挂住。
//!
//! ## 三层
//!
//! ```text
//! mod.rs    能力判断（can_ask）+ 四种提问原语        不知道自己在问什么
//! pick.rs   「参数给了 / 问 / 问不了就退出」三态骨架   不知道候选集从哪来
//! paths.rs  路径这一种回答的输入设施（拆行、补全）      不知道路径要拿来干什么
//! ```
//!
//! 命令面因此只写**声明**：候选集从哪来、参数怎么解析、菜单每行长什么样、三种措辞
//! 分别说什么。三态本身在 [`pick::Picker`] 里只有一份实现——它此前摊在每条命令里各写
//! 一遍，而漏掉「问不了就退出」那一态的表现是**在管道与 CI 里永久挂起且日志无异常**。

pub mod paths;
pub mod pick;

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::exit::{CliError, CliResult};
use std::sync::atomic::{AtomicBool, Ordering};

/// 用户是否显式禁止了交互（`--no-input`）。
static NO_INPUT: AtomicBool = AtomicBool::new(false);
/// 是否处于结构化输出模式（`--json`）。
static STRUCTURED: AtomicBool = AtomicBool::new(false);

/// 选择菜单一屏最多显示几行，超出的部分翻页。
///
/// **必须设上限**：传输记录与收件箱是只增不减的集合，几百条时菜单会把整个终端顶掉，
/// 用户连自己在选什么都看不见（提示行早已滚出可视区）。dialoguer 在设了上限之后
/// 会自己做翻页与滚动。
///
/// 12 是「翻页时仍看得到提示行」与「一次看得到足够多候选」之间的取值——
/// 常见终端 24 行，菜单之上还有提示与此前命令的输出。
const MENU_MAX_ROWS: usize = 12;

/// 记下本次运行的交互能力。**在分派任何命令之前调用一次。**
///
/// 做成进程级状态而不是逐层传参：这两位是**环境事实**，不是某条命令的参数。
/// 让它们跟着调用链走，等于要求每一个可能提问的函数都多带两个布尔——而漏掉任何一处
/// 的表现是「在不该问的地方问了」，也就是挂住。
pub fn configure(no_input: bool, json: bool) {
    NO_INPUT.store(no_input, Ordering::SeqCst);
    STRUCTURED.store(json, Ordering::SeqCst);
}

/// 此刻能不能向用户提问。
///
/// 四条判据缺一不可：
///
/// - **stdin 是终端**——决定读不读得到回答。
/// - **stderr 是终端**——决定提示看不看得见。只判 stdin 的话，
///   `swarmdrop invite create 2>/dev/null` 会变成一个没有提示的光标，用户不知道程序在等什么。
/// - **未开 `--json`**——那个开关声明的是「调用方是程序」，而程序不会阅读绘制在 stderr 上的
///   选择菜单。即使 stdin 恰好是终端也不问。
/// - **未给 `--no-input`**——TTY 检测有测不准的场景（部分 CI 会分配伪终端），
///   显式开关是脚本与 agent 的逃生口。
pub fn can_ask() -> bool {
    !NO_INPUT.load(Ordering::SeqCst)
        && !STRUCTURED.load(Ordering::SeqCst)
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
}

/// 用户是否**显式**关掉了交互（`--no-input` 或 `--json`），而不是环境碰巧不支持。
///
/// 两者要分开：显式关掉是一个**明确的意图**，命令应当照常运行、只是不问人（例如
/// `invite create --no-input` 生成邀请并守着，期间到达的配对请求一律拒绝）；
/// 而「环境问不了人且用户什么也没说」意味着这条命令注定做不成它该做的事，应当直接
/// 以用法错误退出，而不是默默地把每一个入站请求都拒掉。
pub fn interaction_declined() -> bool {
    NO_INPUT.load(Ordering::SeqCst) || STRUCTURED.load(Ordering::SeqCst)
}

/// 全部提示共用的主题。
///
/// **只有这一处构造主题**：菜单、确认框、输入框在同一次交互里可能连着出现（`send`
/// 就是「选设备 → 输入路径」两屏），主题各写各的会让同一条命令里的选中标记与提示前缀
/// 长得不一样。
///
/// 颜色由 `console` 按环境自行降级（非终端、`NO_COLOR`、`TERM=dumb` 时自动关掉），
/// 所以这里无条件用彩色主题不会污染管道——何况本模块只在 [`can_ask`] 为真时才提问。
fn theme() -> dialoguer::theme::ColorfulTheme {
    dialoguer::theme::ColorfulTheme::default()
}

/// 提示绘制在哪。
///
/// stderr——它是过程信息，不是命令的结果。stdout 在结构化模式下只能有最终结果。
fn term() -> dialoguer::console::Term {
    dialoguer::console::Term::stderr()
}

/// 此刻问不了人就以用法错误退出。
///
/// 三个提问入口（两处菜单 + 行输入）各写一遍这三行时，漏掉任何一处的表现都是
/// **在管道与 CI 里永久挂起且日志无异常**——`dialoguer` 会去读一个永不到来的 stdin。
/// 收成一处之后，「该不该问」只有一个答案。
pub(crate) fn require_can_ask(unavailable: &str) -> CliResult<()> {
    if can_ask() {
        Ok(())
    } else {
        Err(CliError::Usage(unavailable.into()))
    }
}

/// 在阻塞线程上跑一次 dialoguer 交互。
///
/// 两件事收在这里，四个提问原语因此各自只剩 dialoguer 那几行：
///
/// - **`spawn_blocking`**：dialoguer 是同步阻塞的，直接在异步运行时里调会卡住整个
///   worker——而本进程的节点、传输与本地通道全都跑在同一个运行时上。
/// - **`None` 的统一语义**：join 失败（任务 panic）与 dialoguer 自己返回的 `None`
///   （用户按了 Esc / Ctrl-C、终端没了）对调用方是同一件事——**读不到回答**。
async fn on_terminal<T: Send + 'static>(
    ask: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(ask).await.ok().flatten()
}

/// 问一个是 / 否，默认否。
///
/// **默认必须是否**：这个提问的用途是守住配对这道信任边界、以及拦住不可逆的批量撤销，
/// 而用户对一个看不懂的提示最可能的反应是直接回车。回车放行等于没问。
///
/// 返回 `None` = **读不到回答**（终端被关掉、管道断开、用户按了 Ctrl-C）。
/// 它与 `Some(false)` 刻意分开：两者都不该放行，但后续动作不同——明确的「否」意味着
/// 「拒了这个、继续等下一个」，而读不到回答意味着「这个人已经走了」，此时还打印一句
/// 「仍在等待」只会与紧随其后的中止自相矛盾。
pub async fn confirm(prompt: String) -> Option<bool> {
    on_terminal(move || {
        dialoguer::Confirm::with_theme(&theme())
            .with_prompt(prompt)
            .default(false)
            .show_default(true)
            .interact_on(&term())
            .ok()
    })
    .await
}

/// 让用户从若干项里挑一个，返回选中项的下标。
///
/// 返回 `None` = 读不到回答（同 [`confirm`]）。**没有默认选中项**：这个菜单的用途是
/// 补全一个不可逆操作的目标（撤销哪张邀请、解除哪台设备），预选一项等于在用户直接
/// 回车时替他做了那个决定。
///
/// 菜单绘制在 stderr——它是过程信息，不是命令的结果。
///
/// 私有：命令面一律经 [`pick::Picker`] 用它，那里才有「该不该问」的三态判断。
async fn select(prompt: String, items: Vec<String>) -> Option<usize> {
    on_terminal(move || {
        dialoguer::Select::with_theme(&theme())
            .with_prompt(prompt)
            .items(&items)
            .max_length(MENU_MAX_ROWS)
            .interact_on_opt(&term())
            .ok()
            .flatten()
    })
    .await
}

/// 让用户勾选若干项，返回选中项的下标（可能为空）。
///
/// **键位必须写进提示语**：多选菜单画出来与单选几乎一样，而它要按空格勾选——
/// 不说的话用户会直接回车，得到一个空选择，然后以为命令什么都没做。
/// 由 [`select_many`] 统一附加，不交给调用方各写各的。
///
/// 返回 `None` = 读不到回答（用户按了 Esc / Ctrl-C，或终端没了）。
/// 与「勾了零项后回车」刻意分开：后者是一个明确的「我看过了，不选」。
///
/// 私有，理由同 [`select`]。
async fn select_many(prompt: String, items: Vec<String>) -> Option<Vec<usize>> {
    let prompt = format!("{prompt}（空格勾选 · 回车确认）");
    on_terminal(move || {
        dialoguer::MultiSelect::with_theme(&theme())
            .with_prompt(prompt)
            .items(&items)
            .max_length(MENU_MAX_ROWS)
            .interact_on_opt(&term())
            .ok()
            .flatten()
    })
    .await
}

/// 一个要用户敲字的问题。
///
/// 做成 builder 而不是几个平行函数：三处调用点（发送的路径、邀请串、导出目录）
/// 在「允不允许空」「有没有默认值」「要不要路径补全」上各是一种组合，
/// 平行函数会立刻长成三个几乎一样的实现。
pub struct Question {
    prompt: String,
    default: Option<String>,
    allow_empty: bool,
    complete_paths: bool,
    echo: bool,
}

impl Question {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            default: None,
            allow_empty: false,
            complete_paths: false,
            echo: true,
        }
    }

    /// 直接回车时用这个值。
    pub fn default(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// 允许空回答。
    ///
    /// 只有「循环收集，空行结束」那类问题需要它——其余场合空回答等于没回答。
    pub fn allow_empty(mut self) -> Self {
        self.allow_empty = true;
        self
    }

    /// 开启 Tab 路径补全。
    ///
    /// 交互输入框里 shell 不介入，**没有补全的路径输入在真实终端里几乎不可用**：
    /// 用户要么原样手敲一条长路径，要么改用拖拽（那又会带上 shell 转义，见
    /// [`paths::split_line`]）。
    pub fn complete_paths(mut self) -> Self {
        self.complete_paths = true;
        self
    }

    /// 回车后不要把回答再复述一遍。
    ///
    /// 默认复述（dialoguer 的行为），它留下「你刚才答了什么」的痕迹。**但回答会被后续
    /// 输出复述时必须关掉**：发送的路径紧接着会逐条回显成 `+ …`，导出目录紧接着出现在
    /// 「已导出 N 项到 …」里——不关的话同一条长路径在屏幕上连着出现两次，
    /// 而路径长到会折行时，那两次看起来像是「加了两遍」。
    pub fn no_echo(mut self) -> Self {
        self.echo = false;
        self
    }

    /// 问出去，问不了就以用法错误退出。
    ///
    /// 与 [`pick::Picker`] 同一套三态：`unavailable` 说清「怎么用参数给出这个值」。
    ///
    /// 回答**一律 trim**。终端里读回来的一行几乎必然带上首尾空白（粘贴时的换行、软换行
    /// 留下的空格），而本 crate 问的三种东西——邀请链接（base32 载荷）、文件路径、目录
    /// ——没有一种以空白为有效内容，trim 不会误伤任何合法输入。让每个调用点各自记得
    /// trim 的话，漏掉是静默的：`inbox export` 少一次就会拿到带尾随空白的目录名，
    /// 而报错指向的那个路径在屏幕上看起来完全正常。
    pub async fn ask(self, unavailable: &str) -> CliResult<String> {
        require_can_ask(unavailable)?;

        let Self {
            prompt,
            default,
            allow_empty,
            complete_paths,
            echo,
        } = self;

        // 闭包会把 `default` move 进去，这里先留一份给上面那次「trim 后回落」。
        let fallback = default.clone();
        let answer = on_terminal(move || {
            // 主题与补全器都要活到 `interact` 之后——`with_theme` / `completion_with`
            // 借的是它们，而这里的构造分了好几条语句（其余三种提问是单条链式表达式，
            // 临时值活到语句末尾就够）。
            let theme = theme();
            let completion = paths::PathCompletion;
            let mut input = dialoguer::Input::<String>::with_theme(&theme)
                .with_prompt(prompt)
                .allow_empty(allow_empty)
                .report(echo);
            if let Some(value) = default {
                input = input.default(value);
            }
            if complete_paths {
                input = input.completion_with(&completion);
            }
            // `interact_text_on` 而非 `interact_on`：前者是逐键读的行编辑器
            // （方向键、Tab 补全都靠它），后者只做一次 `read_line`，Tab 会原样进到字符串里。
            input.interact_text_on(&term()).ok()
        })
        .await;

        // 读不到回答 ⇒ 用户中止（Ctrl-C 走的就是这条）。
        let line = answer.ok_or(CliError::Aborted)?.trim().to_owned();

        // ⚠️ **trim 之后要再回落一次默认值**：dialoguer 的 `default` 只在回答**完全为空**
        // 时生效，而用户敲一个空格再回车时它看到的是 `" "`——非空，于是既不用默认值、
        // 也过得了 `allow_empty(false)`。trim 完就成了空串，一路走到「导出到当前目录」
        // 且提示里的目标是一片空白。
        Ok(match (line.is_empty(), fallback) {
            (true, Some(value)) => value,
            _ => line,
        })
    }

    /// 问一条**路径**。
    ///
    /// 与 [`Self::ask`] 的区别不只是返回类型：它把回答按 shell 规则解码
    /// （去引号与转义、展开 `~`），而那正是 `complete_paths` 写回去的形式。
    ///
    /// **两者必须成对**，所以这里不给调用方拆开的机会：`complete_paths()` 开了补全却
    /// 只调 `ask()`，拿到的就是一条**还带着反斜杠**的路径——`inbox export` 一度就是这样，
    /// 而症状只在目录名含空格时才出现。
    pub async fn ask_path(self, unavailable: &str) -> CliResult<PathBuf> {
        let line = self.complete_paths().ask(unavailable).await?;
        Ok(paths::parse_one(&line))
    }

    /// 问**若干条**路径（一行给多个，见 [`paths::parse`]）。
    ///
    /// 允许空回答：这类提问的形态是「逐行收集、空行结束」，由调用方判断空表示什么。
    pub async fn ask_paths(self, unavailable: &str) -> CliResult<Vec<PathBuf>> {
        let line = self.complete_paths().allow_empty().ask(unavailable).await?;
        Ok(paths::parse(&line))
    }
}

/// 测试期间独占交互状态，并在结束时**一定**复位。
///
/// 两件事都必须有：
///
/// - **串行化**。[`configure`] 写的是进程级状态，而 `cargo test` 默认并行——两个测试
///   同时改它，断言看到的就是对方的设置。
/// - **复位**。写在测试体末尾的复位语句在 `assert!` / `expect()` 失败时**跑不到**，
///   于是一个失败的测试会把 `NO_INPUT=true` 泄漏给同进程的下一个——那个测试随后以
///   一种与自己无关的方式失败。放进 `Drop` 才覆盖 panic 路径。
///
/// 用 tokio 的锁而不是 `std::sync::Mutex`：持有期要跨 `await`（被串行化的测试体里就有
/// 超时等待），而 std 的 guard 跨 await 会阻塞整个 worker 线程。
#[cfg(test)]
pub struct InteractionGuard(
    // 只为持有到 `Drop`——锁本身没有可读的内容。
    #[expect(dead_code, reason = "持有即独占")] tokio::sync::MutexGuard<'static, ()>,
);

#[cfg(test)]
impl Drop for InteractionGuard {
    fn drop(&mut self) {
        configure(false, false);
    }
}

/// 进入「问不了人」的状态并独占它，直到 guard 被丢弃。
#[cfg(test)]
pub async fn no_interaction() -> InteractionGuard {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let guard = InteractionGuard(LOCK.lock().await);
    configure(true, false);
    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--no-input` 与 `--json` 各自都足以一票否决交互。
    ///
    /// 这条看守的是两个独立的承诺：前者是脚本的逃生口（TTY 检测会测不准），
    /// 后者是「调用方是程序」的声明。任何一个失效，管道里的命令都会去等一个
    /// 永不到来的回答——表现是永久挂起且日志无异常。
    // async 只为拿那把串行锁（见 `interaction_test_guard`），测试本身不涉及异步。
    #[tokio::test]
    async fn either_flag_disables_interaction() {
        // guard 进来就把 `--no-input` 打开了，这里再逐个验证两个开关各自的效力。
        let _guard = no_interaction().await;
        assert!(!can_ask(), "--no-input 必须禁用交互");

        configure(false, true);
        assert!(!can_ask(), "--json 必须禁用交互");
    }

    /// 问不了人时立刻以用法错误退出，绝不去读 stdin。
    ///
    /// 这条看守 [`Question`] 那一份 `can_ask` 判断（菜单那两份在 [`pick`] 里各有一条）。
    /// 漏掉任何一份的表现都是**永久挂起且日志无异常**——在管道与 CI 里只看得到一个超时。
    #[tokio::test]
    async fn asking_without_a_terminal_fails_fast() {
        let _guard = no_interaction().await;

        let line = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            Question::new("敲点什么").ask("请指定"),
        )
        .await
        .expect("Question::ask 挂起了");

        assert_eq!(
            line.expect_err("应当报用法错误").code(),
            crate::exit::Code::Usage
        );
    }
}
