//! 发送结果渲染。

use std::path::Path;

use crate::runtime::transfer::{SendOutcome, TextOutcome};

/// 文件发送结果。
///
/// **走 [`crate::runtime::transfer::file_payload`] 转成 JSON 再交给 [`render_from_json`]，不另写一份文案。**
/// 两条取数路径（自持临时节点 / 常驻节点）此前各拼一遍同一句话——改了一处另一处会静默
/// 保持旧的，**而用户看到哪一句取决于此刻有没有常驻节点在跑**。文本那支从一开始就是
/// 一份共用的（见 [`crate::runtime::transfer::text_payload`] 的注释），这里补齐。
pub fn render(outcome: &SendOutcome, json: bool) {
    render_from_json(&crate::runtime::transfer::file_payload(outcome), json);
}

/// 结果来自通道对面的常驻节点时，它已经是 JSON。
pub fn render_from_json(payload: &serde_json::Value, json: bool) {
    if json {
        println!("{payload}");
        return;
    }
    println!(
        "已发送 {} 个文件（{}）",
        super::int_or_zero(payload, "fileCount"),
        human_bytes(
            payload
                .get("totalBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        )
    );
}

/// 文本送达的回执。
///
/// 措辞是**「已送达」而不是「已发送」**，这不是修辞：底层 RPC 的成功应答代表接收端
/// 已经把正文落库，说「已发送」会把一个确定的事实降级成一个不确定的事实，而失败那几支
/// 恰恰都长着「发出去了但没到」的样子（spec: text-send-experience）。
pub fn render_text(outcome: &TextOutcome, json: bool) {
    render_text_from_json(&crate::runtime::transfer::text_payload(outcome), json);
}

/// 结果来自通道对面的常驻节点时，它已经是 JSON。
pub fn render_text_from_json(payload: &serde_json::Value, json: bool) {
    if json {
        // 打成**一行**，而不是走 `render::emit_json` 的缩进形态：`swarmdrop send` 的两种
        // 内容在 `--json` 下不该长得不一样，而结果行是一行时 `read` 之类的按行工具才用得上。
        println!("{payload}");
        return;
    }
    println!(
        "已送达 {}（{}）",
        super::text_or(payload, "peerName", "对端"),
        human_bytes(payload.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0))
    );
}

/// 等待对端接收文本时的转轮。
///
/// **它不是装饰。** 对端策略要求人工确认时（新配对设备的默认档位就要求），这次调用会
/// 一直等到对方点确认或确认窗口耗尽——最长 5 分钟。没有它，屏幕上是一条已经敲下回车、
/// 却几分钟不动也不报错的命令，用户唯一能做的判断是「卡死了」，然后按 Ctrl-C。
///
/// 与 [`Progress`] 同样画在 stderr、同样在 `Drop` 里清行、同样以「非终端自动静默」
/// 交给 indicatif——**不要用一句 `eprintln!` 代替**：那行字在传输结束后仍留在屏幕上，
/// 与紧随其后的结果自相矛盾（「等待接收」后面跟着「已送达」）。
pub struct Waiting(indicatif::ProgressBar);

impl Waiting {
    /// 等待对端接收一条文本。
    ///
    /// `enabled` 为假（结构化输出模式）时返回一个不绘制任何东西的实例，理由同
    /// [`Progress::new`]。
    pub fn new(enabled: bool, peer_name: &str) -> Self {
        Self::with_message(
            enabled,
            format!("等待 {peer_name} 接收（对端可能需要人工确认）"),
        )
    }

    /// 文件正由**常驻节点**发送时的转轮。
    ///
    /// 这条路径此前什么都不画，依据是「文件那支自己有真进度条」——**那句话只对本地
    /// 临时节点成立**。交给常驻节点做时，准备与传输的进度事件都在**它那个进程**里，
    /// 客户端这边从敲下回车到传完一个字都没有。用户看到的就是「输入完文件后卡住」。
    ///
    /// **这段等待很短**：常驻节点一开始算校验和就会推第一帧进度过来，转轮随即让位给
    /// [`Preparing`]（见 [`RemoteProgress`]）。它盖住的只是「请求发出 → 第一帧回来」
    /// 之间那段——枚举目录、解析目标、连上对端。
    ///
    /// 措辞因此不提 `transfer watch`：进度就在这个终端上，不必让用户另开一个。
    pub fn sending(enabled: bool, peer_name: &str) -> Self {
        Self::with_message(enabled, format!("正在发送到 {peer_name}…"))
    }

    /// 准备已完成，正在等对端点「接受」。
    ///
    /// 与 [`Self::sending`] 分开是因为两段等的是不同的东西：那条等的是「请求发出到
    /// 第一帧回来」（枚举、解析目标、连上对端，通常一两秒），这条等的是**一个人的动作**
    /// ——最长到 `OFFER_RESPONSE_TIMEOUT_SECS`（180 秒）。措辞必须让用户知道球在对面。
    pub fn awaiting_accept(enabled: bool, peer_name: &str) -> Self {
        Self::with_message(enabled, format!("已准备好，等待 {peer_name} 接受…"))
    }

    fn with_message(enabled: bool, message: String) -> Self {
        let spinner = if enabled {
            indicatif::ProgressBar::new_spinner()
        } else {
            indicatif::ProgressBar::hidden()
        };
        spinner.set_message(message);
        spinner.enable_steady_tick(SPINNER_TICK);
        Self(spinner)
    }
}

/// 常驻节点在做时，客户端这一侧的进度渲染。
///
/// 通道对面推来的是**事实**（阶段 + 已完成 + 总量），画成什么样由这一侧决定——于是两条
/// 路径（自持节点 / 常驻节点）用的是同一组进度条，不会各自漂移出一套样子。
///
/// 三个状态按顺序推进，**同一时刻只有一个占着 stderr**：
///
/// | 状态 | 何时 | 画什么 |
/// |---|---|---|
/// | 等待 | 请求发出，第一帧还没回来 | 转轮 |
/// | 准备 | 收到 `preparing` 帧 | [`Preparing`] |
/// | 传输 | 收到 `transferring` 帧 | [`Progress`] |
///
/// ⚠️ **切换必须先收掉上一个**（靠 `Option::take` 触发 `Drop`）。两条 indicatif 同时活着
/// 会互相擦掉对方的行——表现是进度条闪烁、残影，而且只在真终端里显形，管道与 CI 全绿。
pub struct RemoteProgress {
    enabled: bool,
    /// 准备满格后那句「等待对端接受」要带上它。
    peer_name: String,
    waiting: Option<Waiting>,
    preparing: Option<Preparing>,
    transferring: Option<Progress>,
}

impl RemoteProgress {
    /// 开始等待。`enabled` 为假（结构化输出模式）时全程什么都不画。
    pub fn new(enabled: bool, peer_name: &str) -> Self {
        Self {
            enabled,
            peer_name: peer_name.to_owned(),
            waiting: Some(Waiting::sending(enabled, peer_name)),
            preparing: None,
            transferring: None,
        }
    }

    /// 收到通道推来的一帧。
    ///
    /// **不认识的阶段一律忽略**，不报错也不清屏：这条帧是内部通道上的自家格式，
    /// 对不上只可能是两端版本不一致（用户升级了 CLI 但常驻节点还是旧的）——那时
    /// 少画一个进度条是可接受的降级，把一次正常的传输打断不是。
    pub fn on_frame(&mut self, payload: &serde_json::Value) {
        match payload.get("phase").and_then(serde_json::Value::as_str) {
            Some(crate::runtime::transfer::PHASE_PREPARING) => {
                let done = field(payload, "done");
                let total = field(payload, "total");
                // **准备满格 ⇒ 换回转轮，别停在一个 100% 的条上。**
                //
                // 准备结束之后还要等对端接受，最长 180 秒（`OFFER_RESPONSE_TIMEOUT_SECS`），
                // 而传输段的第一帧要到对方点了确认才有。一条走满的进度条在那儿停三分钟，
                // 与「卡住」是同一个样子——正是这套进度要消灭的那个症状。
                // 转轮会动，且这句话说的是实话：在等对端。
                if total > 0 && done >= total {
                    self.preparing = None;
                    self.transferring = None;
                    self.waiting.get_or_insert_with(|| {
                        Waiting::awaiting_accept(self.enabled, &self.peer_name)
                    });
                    return;
                }
                // 第一帧到了：转轮让位。**传输段也一并收掉**——目前服务端保证
                // 准备一定先于传输，所以这一行是恒等的；但「同一时刻只有一个占着 stderr」
                // 是本类型自己的不变量，让它在本地成立，而不是依赖远端的推进顺序。
                self.waiting = None;
                self.transferring = None;
                let bar = self
                    .preparing
                    .get_or_insert_with(|| Preparing::new(self.enabled));
                bar.update(
                    field(payload, "done"),
                    field(payload, "total"),
                    payload
                        .get("file")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    field(payload, "completedFiles") as u32,
                    field(payload, "totalFiles") as u32,
                );
            }
            Some(crate::runtime::transfer::PHASE_TRANSFERRING) => {
                // 准备段结束，让它先收干净再画传输段。
                self.waiting = None;
                self.preparing = None;
                let bar = self
                    .transferring
                    .get_or_insert_with(|| Progress::new(self.enabled));
                bar.update(field(payload, "done"), field(payload, "total"));
            }
            _ => {}
        }
    }
}

/// 从进度帧里取一个整数字段；缺失或不是数字时为 0。
///
/// 缺失给 0 而不是跳过整帧：进度是可以不准的，但**进度条必须继续动**——
/// 一个因为少了个字段就停住的进度条，与「卡住」是同一种表现。
fn field(payload: &serde_json::Value, key: &str) -> u64 {
    payload
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// 转轮的重画间隔。
///
/// 这条等待没有任何进度可言（对端确认与否是一个二值事件），转轮唯一的职责是证明
/// 进程还活着，所以只要快到看得出在动即可。
const SPINNER_TICK: std::time::Duration = std::time::Duration::from_millis(120);

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

/// 交互收集路径时，回显刚加进来的一条。
///
/// **走 stderr**：它是过程信息，不是命令结果。逐条回显是为了让用户看清**拆行拆对了没有**
/// ——路径里的空格是那一步最容易出错的地方（拖进终端时它是 `\ `，粘贴时可能是裸空格）。
pub fn echo_added(path: &Path) {
    eprintln!("  + {}", path.display());
}

/// 同上，回显一条没能用的。
///
/// **只报这一条、不中断整轮**：一次拖五个文件错一个，让用户重来五次不合理。
pub fn echo_rejected(err: &crate::exit::CliError) {
    eprintln!("  ! {err}");
}

/// 人类可读的字节数。
///
/// 用 1024 进制并标注 KiB/MiB：文件大小在磁盘与传输语境下都是二进制单位，
/// 用 1000 进制会与用户在文件管理器里看到的数对不上。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 给一套样式接上 `{done}` / `{total}` / `{rate}` 三个 key，全部经 [`human_bytes`]。
///
/// **不用 indicatif 自带的 `{binary_bytes}`**：它给两位小数（`1.00 MiB`），而同一屏里
/// 的结果行走 [`human_bytes`]（一位小数，`1.0 MiB`）——同一个数在同一屏里两种写法。
///
/// 抽出来是因为有两个消费者：`send` 的单条进度条与 `transfer watch` 的面板。
/// 各注册一遍的话，改了其中一处的单位写法，另一处会**静默**保持旧样子。
pub fn with_byte_keys(style: indicatif::ProgressStyle) -> indicatif::ProgressStyle {
    style
        .with_key(
            "done",
            |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = write!(w, "{}", human_bytes(state.pos()));
            },
        )
        .with_key(
            "total",
            |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = write!(w, "{}", human_bytes(state.len().unwrap_or(0)));
            },
        )
        .with_key(
            "rate",
            |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = write!(w, "{}/s", human_bytes(state.per_sec() as u64));
            },
        )
}

/// 准备阶段（校验和 + bao 验签树）的进度条。
///
/// **必须与 [`Progress`] 视觉可区分**，这是跨端契约而不是措辞偏好：准备是本机在算、
/// 一个字节都还没上网，传输是字节真的在动。两者共用同一个视觉原语，正是那次
/// 「用户把 1.99 GB 的准备读成传输、进而把新会话读成『续传从 0% 重来』」的直接原因
/// （见 `crates/transfer/src/flow/prepare.rs` 的 `PROGRESS_THROTTLE` 注释）。
/// 桌面与移动端用颜色区分（灰 = 准备中，teal = 传输中），终端这一份用**动词**区分。
///
/// **不显示速率与剩余时间**：那两个数在传输语境下回答的是「还要等多久下载完」，而这一段
/// 是本地磁盘读 + 哈希，速率取决于磁盘而非网络。把它画得跟传输一模一样只会加深上面那个误读。
pub struct Preparing(indicatif::ProgressBar);

/// 准备阶段的模板。**动词与 [`TEMPLATE`] 不同**，理由见 [`Preparing`]。
///
/// 与 `TEMPLATE` 同为常量、同一条理由：写错时只是静默退化成默认样式。
const PREPARE_TEMPLATE: &str = "准备中 {bar:24} {percent:>3}%  {done}/{total}  {msg}";

impl Preparing {
    /// `enabled` 为假（结构化输出模式）时返回一个不绘制任何东西的实例，理由同
    /// [`Progress::new`]。
    pub fn new(enabled: bool) -> Self {
        let bar = if enabled {
            indicatif::ProgressBar::no_length()
        } else {
            indicatif::ProgressBar::hidden()
        };
        bar.set_style(with_byte_keys(
            indicatif::ProgressStyle::with_template(PREPARE_TEMPLATE)
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
        ));
        Self(bar)
    }

    /// 收到一条准备进度事件。
    ///
    /// **文件名只在多文件时显示**：只发一个文件时它已经写在用户刚敲的命令里，
    /// 再占一行只是把同一件事说两遍；而多文件时「现在在算哪个」才是这条进度唯一
    /// 回答得了的问题。
    pub fn update(
        &self,
        bytes_hashed: u64,
        total_bytes: u64,
        current_file: &str,
        completed_files: u32,
        total_files: u32,
    ) {
        self.0.set_length(total_bytes);
        self.0.set_position(bytes_hashed);
        if total_files > 1 {
            // `completed_files` 是**已完成**的数量，所以正在算的那个是它 +1。
            // 直接用它会让一批 3 个文件的准备停在「2/3」上收尾。
            let at = (completed_files + 1).min(total_files);
            self.0
                .set_message(format!("{at}/{total_files}  {current_file}"));
        }
    }
}

/// 离开作用域即收掉，理由同 [`Progress`] 的 `Drop`。
///
/// 这一条尤其不能漏：准备之后紧接着是传输进度条，两条同时占着 stderr 会互相擦掉。
impl Drop for Preparing {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

/// 传输进度条。
///
/// **画在 stderr**：结构化模式下 stdout 只能有最终结果，而人类可读模式下把进度混进
/// 结果也不利于管道使用。
///
/// 走 `indicatif` 而不是自己 `\r` 刷新，换来的是三件自写版本没有的事：
///
/// - **非终端时自动静默**。自写版本在 `swarmdrop send … | tee log` 或 CI 里照样输出
///   回车控制符，日志文件里于是变成一行几百个 `\r` 拼起来的乱码。
/// - **速率与剩余时间**。传大文件时这两个数才是用户真正在等的答案，
///   而它们要维护一个时间窗口，不是「再加一行 format!」能顺手做对的。
/// - **重绘时清行**。stderr 上还有 tracing 的日志，自写版本被日志插一行后会留下
///   半截残影，直到下一次刷新才被覆盖。
pub struct Progress(indicatif::ProgressBar);

/// 进度条模板。
///
/// **必须是常量**：模板写错时 [`Progress::new`] 回退到默认样式，进度条照常出现、
/// 只是速率与剩余时间不见了，没有任何报错。看守它的测试如果自己抄一份字面量，
/// 改坏这里、忘了改那里，测试仍然绿——那条护栏就等于不存在。
/// **不用 indicatif 的 `{binary_bytes}`**：它给两位小数（`1.00 MiB`），而同一条 `send`
/// 结束时打印的结果行走 [`human_bytes`]（一位小数，`1.0 MiB`）——同一个数在同一屏里
/// 两种写法。改用自定义的 key 把两处都接到 `human_bytes` 上。
const TEMPLATE: &str = "传输中 {bar:24} {percent:>3}%  {done}/{total}  {rate}  剩余 {eta}";

impl Progress {
    /// `enabled` 为假（结构化输出模式）时返回一个不绘制任何东西的实例。
    ///
    /// 不用 `Option<Progress>` 是刻意的：调用点会因此散落 `if let Some(..)`，
    /// 而「什么时候该画」必须只回答一次。stderr 不是终端时由 indicatif 自己隐藏。
    pub fn new(enabled: bool) -> Self {
        let bar = if enabled {
            indicatif::ProgressBar::no_length()
        } else {
            indicatif::ProgressBar::hidden()
        };
        bar.set_style(with_byte_keys(
            indicatif::ProgressStyle::with_template(TEMPLATE)
                // 写错只会在运行时退化成默认样式——由 `progress_template_is_valid` 钉住。
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
        ));
        Self(bar)
    }

    /// 收到一条进度事件。
    ///
    /// 总量每次都重设：它由第一条事件才带过来，而续传场景下同一会话的总量可能变化。
    pub fn update(&self, transferred: u64, total: u64) {
        self.0.set_length(total);
        self.0.set_position(transferred);
    }
}

/// 离开作用域即收掉进度条。
///
/// **不做成 `finish()` 方法在每个返回点各调一次**：等待终态的那个循环有四条出口
/// （完成、失败、拒绝、通道断开），只有第一条会自然想起要收尾——旧版本就漏了另外三条，
/// 于是「传输失败: …」直接印在那条没有换行的进度行后面。新增一种终态时同样会漏。
///
/// `finish_and_clear` 而非 `finish`：最终结果紧接着打印在 stdout 上，
/// 留一条走完的进度条在旁边只是把同一个数字再说一遍。
impl Drop for Progress {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {

    /// **准备满格之后不能停在一个 100% 的条上。**
    ///
    /// 准备结束到对端接受之间最长 180 秒。一条走满的进度条在那儿停三分钟，与「卡住」
    /// 是同一个样子——而这套远端进度存在的全部理由就是消灭那个样子。满格后要换回一个
    /// 会动、且说的是实话（在等对端）的转轮。
    #[test]
    fn a_full_prepare_bar_gives_way_to_the_waiting_spinner() {
        let mut remote = RemoteProgress::new(false, "台式机");

        remote.on_frame(&serde_json::json!({
            "phase": crate::runtime::transfer::PHASE_PREPARING,
            "done": 10, "total": 100, "file": "a.bin",
            "completedFiles": 0, "totalFiles": 1,
        }));
        assert!(remote.preparing.is_some(), "进行中应当画准备条");
        assert!(remote.waiting.is_none(), "进行中不该是转轮");

        remote.on_frame(&serde_json::json!({
            "phase": crate::runtime::transfer::PHASE_PREPARING,
            "done": 100, "total": 100, "file": "a.bin",
            "completedFiles": 1, "totalFiles": 1,
        }));
        assert!(remote.preparing.is_none(), "满格后准备条该收掉");
        assert!(remote.waiting.is_some(), "满格后该换成等待转轮");

        // 传输段到了要盖过转轮。
        remote.on_frame(&serde_json::json!({
            "phase": crate::runtime::transfer::PHASE_TRANSFERRING,
            "done": 1, "total": 100,
        }));
        assert!(remote.transferring.is_some(), "传输段该画进度条");
        assert!(remote.waiting.is_none(), "传输开始后转轮该收掉");
    }

    /// `total` 为 0（零字节文件）时不该被当成「满格」。
    ///
    /// `0 >= 0` 成立，少了那道 `total > 0` 的闸，第一帧就会直接跳去等待转轮，
    /// 准备段的进度再也画不出来。
    #[test]
    fn a_zero_total_frame_is_not_mistaken_for_completion() {
        let mut remote = RemoteProgress::new(false, "台式机");
        remote.on_frame(&serde_json::json!({
            "phase": crate::runtime::transfer::PHASE_PREPARING,
            "done": 0, "total": 0, "file": "", "completedFiles": 0, "totalFiles": 0,
        }));
        assert!(remote.preparing.is_some(), "零字节被误判成准备完成");
    }
    use super::*;

    /// 进度条模板必须可解析。
    ///
    /// [`Progress::new`] 在模板写错时回退到默认样式——那是**静默**降级：进度条照常
    /// 出现，只是速率与剩余时间不见了，而没有人会因此收到报错。
    #[test]
    fn progress_template_is_valid() {
        assert!(
            indicatif::ProgressStyle::with_template(TEMPLATE).is_ok(),
            "模板无法解析，Progress::new 会静默退回默认样式"
        );
    }

    /// 结构化输出模式下一个字节都不能画——stdout 的解析方会被进度条冲掉。
    #[test]
    fn structured_mode_draws_nothing() {
        assert!(Progress::new(false).0.is_hidden());
    }

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    }
}
