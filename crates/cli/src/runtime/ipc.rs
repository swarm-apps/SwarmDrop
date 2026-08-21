//! 本地通道：其余命令经它复用正在运行的节点。
//!
//! **这是内部机制，不是对外 API**。两端都是本 crate 的代码，动词集就是命令面的映射，
//! 可以随时改。若将来要把能力暴露给外部程序，那时再基于真实需求决定是提升这套通道还是
//! 另起一个面——现在为一个未定的消费者做通用化是投机。
//!
//! 传输走本地套接字（类 Unix 是域套接字、Windows 是命名管道），载荷是**行分隔的 JSON**：
//! 一行一条消息。选它不是因为高效，是因为出问题时可以直接用 `nc` 看——一个内部调试通道
//! 的可读性比它的字节数重要。
//!
//! ## 信任边界
//!
//! **这条通道上没有认证，能连上就等于能指挥这个节点**（启停、列设备、发文件、应答配对
//! 请求）。拦住其他用户的是数据目录的 0700 权限（见 [`crate::adapter::paths`]），不是
//! 通道自己——套接字文件本身按默认权限创建。
//!
//! 因此**同用户下的其他进程可以绕过一切界面上的确认**：比如直接发 `PairRespond` 接受一个
//! 入站配对，而屏幕上不会弹出任何东西。这不是疏忽，是与本仓既有形态一致的取舍
//! （见 `CLAUDE.md`）：那类进程本来就能读走 `identity.json` 里的**明文私钥**并冒充这台
//! 设备，给通道加一次性 token 不会改变实际的攻击面，只会让人误以为它防住了什么。
//!
//! 界面上的配对确认（`crates/cli/src/cmd/invite.rs`）挡的是**远端**——一个抢先扫到码的人。
//! 那条防线由 `pending_id` 只在本机流转这一点保证，与本通道的信任边界是两件事。

use std::path::Path;

use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::exit::{CliError, CliResult, Code};

/// 客户端请求。
///
/// 动词都是具体的、单一用途的，**不做通用的「转发任意调用」**——那会把这层变成一个
/// 需要版本协商的 API。一个命令可以对应多个动词（`pair` 就用了三个：签发、取待确认
/// 请求、送回答复），但每个动词只干一件写得出名字的事。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum Request {
    Status,
    /// 已配对设备清单。
    DeviceList,
    /// 解除与若干台设备的配对。
    ///
    /// **收复数而不是让客户端循环发**：省掉 N-1 次通道往返，并让「最后还剩几台」由
    /// 做事的那一侧给出，而不是客户端从 N 个返回值里自己拼。
    ///
    /// ⚠️ 它**没有**省掉那一侧的文件读写：`unpair` 仍是每台一轮「读 → 改 → 原子写回」。
    /// 要省那个得让核心支持批量 unpair，不在本层。
    ///
    /// 名称到标识的解析仍在客户端完成（那需要候选集，客户端已经取过）。
    DeviceForget {
        /// 节点标识（完整），至少一个。
        peer_ids: Vec<String>,
    },
    /// 生成一张配对邀请。
    ///
    /// **必须由持有节点的那个进程签发**：邀请里带的是签发者的可拨地址，别的进程另起一个
    /// 节点签出来的邀请指向一个即将消失的临时节点。
    InviteCreate,
    /// 以一个邀请完成配对握手。
    InviteUse {
        invite: String,
    },
    /// 本机已发出且未过期的邀请清单。
    InviteList,
    /// 按 capability 哈希撤销若干张邀请。
    ///
    /// 传完整哈希而非用户敲的前缀：前缀要在**当前邀请集合**里解析才谈得上唯一，
    /// 而那个集合客户端已经取过一次了。让服务端再解析一遍等于把同一段逻辑写两份。
    ///
    /// **收复数而不是让客户端循环发**：无常驻节点时每一次撤销都要新开一个数据库连接、
    /// 跑一遍迁移、把整张邀请表读回内存（还带一次 prune 的写事务），逐张发等于把这些
    /// 全做 N 遍。
    ///
    /// 与 [`Self::InviteRevokeAll`] 仍是两个动词：这条撤的是**指定的那几张**，
    /// 那条连本命令列不出来、这一瞬新签发的也一并作废。
    InviteRevoke {
        /// `sha256(capability)` 的小写 hex，至少一个。
        hashes: Vec<String>,
    },
    /// 撤销全部未过期邀请。
    ///
    /// 不用「客户端取列表再逐条撤」代替它：那是 N 次往返，且中途新签发的邀请会漏掉——
    /// 而这条命令服务的正是「不知道哪张泄露了，全撤」。
    InviteRevokeAll,
    /// 长轮询：取走一个待用户确认的入站配对请求。
    ///
    /// **这条请求本身就是「有人在等配对」的信号**——常驻节点靠它判断配对窗口开着没有，
    /// 窗口关着时入站配对一律被拒。因此客户端必须持续轮询，停下来就等于关窗。
    ///
    /// 应答：`Data` 带一个待确认请求；`Ok` 表示本轮没有请求（应立即再问一次）。
    PairWaitNext,
    /// 对一个待确认的入站配对请求作答。
    PairRespond {
        pending_id: u64,
        accept: bool,
    },
    /// 列出收件箱条目，按接收时间倒序。
    ///
    /// `include_archived` 是**服务端修饰**而不是客户端过滤：`InboxItemSummary` 上的字段
    /// 是 `archived_at` 不是 `archived`，在客户端按 JSON 筛的谓词恒真，而两条取数路径
    /// 本来就只请求未归档项——于是这个开关静默无效。判据同
    /// [`crate::runtime::inbox::list`]。
    ///
    /// ⚠️ 带 `#[serde(default)]`：旧客户端发的 `{"verb":"inbox_list"}` 仍要解析得动
    /// （升级 CLI 不会重启常驻节点）。反方向的代价照实说——新客户端 × 旧常驻节点时
    /// 这个字段会被对面忽略，`include_archived: true` 退化成「只有未归档」。
    InboxList {
        #[serde(default)]
        include_archived: bool,
    },
    /// 取一个收件箱条目的详情。
    InboxShow {
        id: String,
    },
    /// 子串检索收件箱。
    ///
    /// **与 [`Self::InboxList`] 是两条而不是一条带可选查询词的**：命中判据、片段生成与
    /// 截断规则都归内核的 `search_inbox_capped`，而 `InboxList` 走的是
    /// `list_inbox_items`——两者返回的甚至不是同一个类型（`InboxSearchHit` 多出命中片段）。
    /// 合成一条只会得到一个两半互不相干的分支。
    ///
    /// `limit` 是 `Option` 而不是带默认值的 `u32`：默认值归内核，宿主自带一个就会长出
    /// 第五个答案（见 [`crate::runtime::inbox::search`]）。
    InboxSearch {
        query: String,
        limit: Option<u32>,
        include_archived: bool,
    },
    /// 传输记录清单。
    TransferList,
    /// 一条传输记录的详情。
    TransferShow {
        id: String,
    },
    /// **未完成**的传输（`phase != Terminal`）。
    ///
    /// 与 [`Self::TransferList`] 分开而不是让客户端过滤：`transfer watch` 每秒问一次，
    /// 而传输历史只增不减——在客户端筛意味着每一次刷新都要把整张表连同全部文件行
    /// 搬过通道。过滤下推到存储端口（`list_unfinished_projections`）之后，
    /// 搬的是「此刻真的没传完的那几条」。
    TransferUnfinished,
    /// 对若干条会话执行同一个运行控制：暂停 / 恢复 / 取消。
    ///
    /// **三个动作共用一个动词**：它们的服务端骨架完全一致（解析标识 → 按方向派生 →
    /// 汇总），拆成三个只会让同一段代码出现三遍。动作本身是负载的一部分，
    /// 不是「转发任意调用」——它是一个封闭的三值枚举。
    ///
    /// **收复数而不是让客户端循环发**：`watch` 里勾选多条是常态，逐条发等于把
    /// 「按方向查一次记录」做 N 遍，且「哪几条成了」会散在 N 个返回值里。
    ///
    /// ⚠️ 这条**必须由持有节点的那个进程执行**：它作用的是常驻节点内存里的活 actor，
    /// 别的进程起一个临时节点，那里面什么都没有。客户端侧的判据见
    /// [`crate::runtime::access::DaemonAccess`]。
    TransferControl {
        action: crate::runtime::transfers::Control,
        /// 会话标识（完整 UUID），至少一个。
        ids: Vec<String>,
    },
    /// 发送文件。**阻塞到传输终态**——客户端期望 `swarmdrop send` 返回时事情已经做完。
    Send {
        /// 源文件/目录的绝对路径。
        paths: Vec<String>,
        /// 目标设备（名称或节点标识）。
        to: String,
    },
    /// 发送一段文本。**阻塞到拿得出确定结论**（最长到对端的确认窗口耗尽）。
    ///
    /// **与 [`Self::Send`] 是两个动词而不是一个带模式位的动词**：服务端要做的事没有
    /// 一步重合（那边枚举、准备、分块、订阅事件，这边只发一次 RPC），合并只会得到一个
    /// 两半互不相干的分支。
    ///
    /// 正文原样走这条通道——它是本机套接字，信任边界见本模块开头。
    SendText {
        /// UTF-8 正文。上限由核心校验（64 KiB），客户端已先校验过一次以便尽早报错。
        body: String,
        /// 目标设备（名称或节点标识）。
        to: String,
    },
    /// 订阅本机发生的事件，**长驻**：一直发非终态帧，直到客户端走开或节点关停。
    ///
    /// 第一帧是基线（整值快照），其后是增量。载荷是订阅面自己定义的窄结构
    /// （[`crate::runtime::watch::event`]），**不是** `CoreEvent`——那个会泄露配对凭证
    /// 与文本正文，而这条流的终点是消费方跨月留存的日志。
    ///
    /// ⚠️ 它是这条通道上唯一**不会很快给出终态**的动词。既有机制刚好都兜得住：
    /// 帧格式复用 [`Frame::Progress`]，客户端复用 [`request_watching`]，
    /// 对端走人由 [`peer_gone`] 那条 select 分支连同整个任务一起取消。
    /// 唯一不能复用的是 [`FrameSink::try_send`] 的三道闸，理由见 [`FrameSink::send`]。
    Subscribe {
        /// 基线里最多带几条收件箱记录。
        ///
        /// `None` = 用订阅面的默认值（`watch::baseline::DEFAULT_INBOX_LIMIT`）。
        /// 默认值归订阅面而不是通道：通道只是搬运，让它也持有一个默认值就会长出第二个
        /// 答案（判据与 [`Self::InboxSearch`] 的 `limit` 同源）。
        inbox_limit: Option<u32>,
    },
    Stop,
}

/// 线上的一帧。
///
/// 这条通道不是严格的一问一答：一次请求的应答是**若干条进度 + 恰好一条终态**。放宽它的
/// 理由是一个具体的体验缺陷——`send` 交给常驻节点做时，准备与传输的事件都在**那个进程**
/// 里，客户端从敲下回车到传完一个字都没有，看起来就是卡住。
///
/// ⚠️ **它与 [`Response`] 分成两个类型是刻意的，别合并。** 合并（给 `Response` 加一个
/// `Progress` 变体）会让**每一个**调用方的 `match` 都被迫处理一种它那里不可能出现的情况
/// ——[`request`] 已经把进度跳掉了。那种 `unreachable!()` 分支是纯粹的噪声，而且下一个人
/// 会认真地去想「这里该怎么办」。分成两层之后，「拿得到的一定是终态」由类型保证。
///
/// ⚠️ **三个终态变体逐字抄了 [`Response`] 一遍，这也是刻意的，同样别「去重」。**
/// 上面那条理由只否掉了「给 `Response` 加 `Progress`」，没有否掉「把三个终态收进一个
/// `Frame::Terminal { #[serde(flatten)] response }`」——那个写法更短，且编译期保证一样强。
/// 不这么做的理由是**线格式**：现在 `Frame` 序列化出来的字节与**旧版** `Response`
/// 逐字相同（同一个 `tag = "kind"`、同一组 `snake_case` 变体名、同一批字段）。
/// 于是「新客户端 × 旧常驻节点」这个真实存在的窗口里（升级 CLI 不会重启常驻节点，
/// 而 `swarmdrop update` 之后尤其如此），旧节点回来的终态帧新客户端仍然解析得动。
/// 套一层 `Terminal` 会在 JSON 里多一层嵌套，那个兼容性当场消失。
/// 由 [`tests::a_terminal_frame_is_byte_identical_to_a_bare_response`] 钉住。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Frame {
    /// **非终态**：处理还在继续。后面**必然**还会有一条终态帧。
    ///
    /// 订阅（[`Request::Subscribe`]）也走它，只是那条的「处理还在继续」可以持续几天——
    /// 终态在节点关停时才来。名字保留 `progress` 是**线格式约束**：它与旧版逐字相同，
    /// 换名字会让「新客户端 × 旧常驻节点」这个真实窗口里的帧解析不动（见本枚举的文档）。
    Progress {
        payload: serde_json::Value,
    },
    Data {
        payload: serde_json::Value,
    },
    Ok,
    Error {
        code: Code,
        message: String,
    },
}

impl Frame {
    /// 终态帧转成响应；进度帧返回 `None`（调用方应当继续读）。
    fn into_terminal(self) -> Result<Response, serde_json::Value> {
        match self {
            Self::Progress { payload } => Err(payload),
            Self::Data { payload } => Ok(Response::Data { payload }),
            Self::Ok => Ok(Response::Ok),
            Self::Error { code, message } => Ok(Response::Error { code, message }),
        }
    }
}

impl From<Response> for Frame {
    fn from(response: Response) -> Self {
        match response {
            Response::Data { payload } => Self::Data { payload },
            Response::Ok => Self::Ok,
            Response::Error { code, message } => Self::Error { code, message },
        }
    }
}

/// 服务端响应。**只有终态**——进度不在这里，见 [`Frame`]。
///
/// 负载用 [`serde_json::Value`] 而非核心的 DTO：核心那些类型只 derive 了 `Serialize`
/// （它们只需单向出到界面），通道却要往返。**这不是「为隔离核心变动而建 DTO 层」**
/// ——那条在 design D1 里明确否决过；这里纯粹是可反序列化性的技术约束，所以只包一层
/// 不透明的 JSON，不复制字段。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    /// 成功且带负载（已是最终形态的 JSON，客户端按输出模式决定怎么呈现）。
    Data { payload: serde_json::Value },
    /// 成功但无负载。
    Ok,
    /// 服务端处理失败。
    ///
    /// **必须带分类**：同一件事（「没有这条记录」）在无常驻节点时由本地路径产出
    /// `CliError::Usage`（退出码 2），经通道时如果只回一个字符串，客户端就只能一律
    /// 按「节点不可用」（退出码 3）处理——于是
    /// `swarmdrop transfer show $id || retry_if_node_down` 的行为取决于**此刻恰好有没有
    /// 常驻节点在跑**。而 spec「退出码区分失败原因」的整个前提是脚本不必解析文本。
    Error { code: Code, message: String },
}

impl Response {
    /// 把一个失败连同它的分类送回客户端。
    ///
    /// **服务端一律走这里**，不要手写 `Response::Error { .. }`：分类是从 [`CliError`]
    /// 自己身上取的（`err.code()`），手填等于给了一次填错的机会，而填错**不报错**——
    /// 客户端只是拿到一个错误的退出码。
    pub fn err(err: CliError) -> Self {
        Self::Error {
            code: err.code(),
            message: err.to_string(),
        }
    }

    /// 服务端自己发现的用法错误（参数格式不对之类），配一句话。
    pub fn usage(message: impl Into<String>) -> Self {
        Self::err(CliError::Usage(message.into()))
    }
}

/// 把路径转成本地套接字名。
fn socket_name(path: &Path) -> CliResult<interprocess::local_socket::Name<'static>> {
    path.to_path_buf()
        .to_fs_name::<GenericFilePath>()
        .map_err(|err| CliError::NodeUnavailable(format!("本地通道路径不可用: {err}")))
}

/// 连上正在运行的节点并发一条请求。
///
/// **连不上不是错误**，是「没有活节点」这一事实——调用方据此决定是自起临时节点还是报错。
/// 因此返回 `Ok(None)`，而不是把「无人应答」和「通道坏了」混成同一个 `Err`。
pub async fn request(socket_path: &Path, req: &Request) -> CliResult<Option<Response>> {
    // 空回调 = 跳过一切进度消息。**所有既有调用方都经这里**，所以服务端新增一条进度
    // 不会噎住任何一个不认识它的命令。
    request_watching(socket_path, req, |_| {}).await
}

/// 同上，但每条进度消息都回调一次。
///
/// 回调是同步的。两类调用方对它的期望正好相反，都是对的：
///
/// - **进度条**（`send`）：只画一帧，不该做任何会 await 的事——那会拖慢读循环，
///   而进度是可以丢的、连接不是。
/// - **订阅**（`watch`）：把这一帧写到 stdout，**慢就该慢**。那次阻塞正是背压，
///   它会顶回服务端的有界队列并在那里变成一次如实上报的截断；偷偷跑掉一帧才是错的。
pub async fn request_watching(
    socket_path: &Path,
    req: &Request,
    mut on_progress: impl FnMut(serde_json::Value),
) -> CliResult<Option<Response>> {
    let name = socket_name(socket_path)?;
    let Ok(stream) = LocalSocketStream::connect(name).await else {
        return Ok(None);
    };

    let mut reader = BufReader::new(stream);
    let mut line = serde_json::to_string(req)
        .map_err(|err| CliError::NodeUnavailable(format!("编码请求失败: {err}")))?;
    line.push('\n');

    reader
        .get_mut()
        .write_all(line.as_bytes())
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("发送请求失败: {err}")))?;

    loop {
        let mut buf = String::new();
        let read = reader
            .read_line(&mut buf)
            .await
            .map_err(|err| CliError::NodeUnavailable(format!("读取响应失败: {err}")))?;
        if read == 0 {
            // 对端在给出终态之前就关了连接（节点被杀 / 崩溃）。
            // **不能当成成功**：调用方会把「没有答案」渲染成一个空结果。
            return Err(CliError::NodeUnavailable(
                "常驻节点在应答之前断开了连接".into(),
            ));
        }

        let frame: Frame = serde_json::from_str(&buf)
            .map_err(|err| CliError::NodeUnavailable(format!("解析响应失败: {err}")))?;

        // 进度是**非终态**：交给调用方看一眼，然后继续读。
        // 不认识它的调用方（`request`）传一个空回调，于是自然地跳过。
        match frame.into_terminal() {
            Ok(terminal) => return Ok(Some(terminal)),
            // **按值交出去**：这一帧已经是本函数解析出来的、没有别人再要的值。
            // 收 `&Value` 会逼订阅那侧为每一条事件克隆一遍，而它是这条通道上唯一的高频
            // 消费者（进度条那侧只读几个字段，按值同样够用）。
            Err(payload) => on_progress(payload),
        }
    }
}

/// 探测：有没有活节点在这条通道后面。
///
/// **不用 pidfile 判活**：PID 会被复用，陈旧 pidfile 会把「没有节点」误判成「有节点」。
/// 能连上才算活着——这是唯一不会误判的判据。
pub async fn is_alive(socket_path: &Path) -> bool {
    let Ok(name) = socket_name(socket_path) else {
        return false;
    };
    LocalSocketStream::connect(name).await.is_ok()
}

/// 等到对端关闭连接。
///
/// 一问一答的协议里，请求读完之后对端不会再发任何字节——所以这里能读到的只有 EOF。
/// **正常情况下它永不完成**，只在连接断开时返回，可以安全地放进 `select!` 的一侧。
async fn peer_gone<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) {
    use tokio::io::AsyncReadExt;

    let mut scratch = [0u8; 1];
    loop {
        match reader.read(&mut scratch).await {
            // EOF 或读失败：两者都意味着这条连接上不会再有人接应答了。
            Ok(0) | Err(_) => return,
            // 协议外的字节。不该出现，但也不是断开的证据——继续盯着。
            Ok(_) => {}
        }
    }
}

/// 处理期间往客户端推送**非终态帧**的出口。
///
/// ## 两个方法，两种投递策略，判据是「调用方是谁」
///
/// | 方法 | 阻塞调用方？ | 丢帧？ | 调用方 |
/// |---|---|---|---|
/// | [`try_send`](Self::try_send) | 绝不 | 会，静默 | `select!` 的**分支体**（传输准备） |
/// | [`send`](Self::send) | 会 | 不会 | 一条订阅**专属**的任务 |
///
/// 这不是「一个安全一个不安全」，是同一件事在两种调用位置上的正确答案各不相同：
///
/// - 分支体挂住 ⇒ 同一个 `select!` 里真正干活的那条 future 得不到轮询，**常驻节点上
///   别人的传输就停在那儿**。所以宁可丢一帧进度——下一帧会纠正它。
/// - 订阅任务挂住 ⇒ 什么都不影响，它本来就只干这一件事；而阻塞正是背压信号，
///   它会一路顶回订阅的有界队列，在那里变成一次**如实上报**的截断
///   （spec: `cli-event-stream` 的「边沿事件不得静默丢失」）。用 `try_send` 反而错：
///   边沿事件会在这里无声消失，而消费方把这段记录跨月留存。
pub struct FrameSink {
    /// 与终态帧**共用**的写端。见 [`IpcServer::accept_one`] 里那段注释。
    writer: std::sync::Arc<tokio::sync::Mutex<DynWriter>>,
    /// 这条流上是否发生过写超时或写失败。见 [`FrameSink::try_send`] 的第 3 道闸。
    poisoned: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

type DynWriter = Box<dyn tokio::io::AsyncWrite + Send + Unpin>;

/// 一帧进度最多允许占用多久。
///
/// 源头每 200ms 才推一帧，所以正常客户端上这次写是微秒级的。超过这个预算只说明对端
/// **连着但不读**（终端按了 Ctrl-S、进程被 SIGSTOP、stderr 管道对面停了），
/// 那时套接字发送缓冲会填满、写永久 Pending。
const PROGRESS_WRITE_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

impl FrameSink {
    /// 尽力推一帧，**永不长时间阻塞调用方**；推不出去就丢掉，不留痕迹。
    ///
    /// ⚠️ 这条约束不是防御性编程，它是正确性要求。调用方是
    /// [`crate::runtime::transfer::prepare_with_progress`] 的 `select!` **分支体**——
    /// 分支体挂住时 `prepare` 那条 future 得不到轮询，于是**常驻节点上真正的哈希计算
    /// 停下来**。一个客户端的终端流控因此能卡住服务端上别人的传输，而事件通道是无界的
    /// （`adapter::events`），期间还会持续涨内存。
    ///
    /// 三道闸，任何一道都只丢这一帧、不影响传输：
    ///
    /// 1. **`try_lock`**：拿不到说明上一帧还在写。进度是可以丢的，排队不是。
    /// 2. **写超时**：见 [`PROGRESS_WRITE_BUDGET`]。
    /// 3. **一次失败即封口**：超时的 `write_all` 可能已经写进去半行，此后再写任何东西
    ///    都会拼在那半行后面，客户端按行解析当场失败——**而那次传输其实是成功的**。
    ///    封口之后连终态帧都不再由本类型写（终态走 `accept_one` 自己那条路径），
    ///    客户端读到 EOF，得到一个诚实的「节点没有应答」，而不是一行拼接出来的乱码。
    pub async fn try_send(&self, payload: serde_json::Value) {
        if self.is_poisoned() {
            return;
        }
        let Some(line) = Self::encode(payload) else {
            return; // 帧序列化失败：丢掉这一帧，不影响传输
        };

        // 拿不到锁 = 上一帧还在写。丢掉这一帧，绝不排队。
        let Ok(mut writer) = self.writer.try_lock() else {
            return;
        };
        match tokio::time::timeout(PROGRESS_WRITE_BUDGET, writer.write_all(line.as_bytes())).await {
            Ok(Ok(())) => {}
            // 超时的那次可能写进去了半行；失败的那次同理。两者都只能封口。
            Ok(Err(_)) | Err(_) => self.poison(),
        }
    }

    /// 推一帧并**等它写完**；返回 `false` 表示这条连接已经不能再写了。
    ///
    /// 给订阅（[`Request::Subscribe`]）用——它的调用方是一条只干这件事的任务，
    /// 判据见本类型的文档。三处与 [`try_send`](Self::try_send) 相反：
    ///
    /// - **等锁**而不是 `try_lock`：订阅连接上只有这一个写者，等不到锁是不可能的；
    ///   真等到了也说明该等。
    /// - **不设写超时**：慢就是背压，它要一路顶回订阅的有界队列变成一次如实的截断，
    ///   在这里偷偷丢掉等于把那条链路的终点挪到一个不上报的地方。
    /// - **写失败要告诉调用方**：订阅是长驻的，不告诉它就会一直往一条死掉的连接上写。
    ///
    /// 封口仍然做：写到一半失败时，后面那条终态帧也不能再写了（同 `try_send` 第 3 道闸）。
    pub async fn send(&self, payload: serde_json::Value) -> bool {
        if self.is_poisoned() {
            return false;
        }
        let Some(line) = Self::encode(payload) else {
            // 序列化失败只可能来自本端的 bug，与连接无关——连接还好着，继续。
            return true;
        };
        if self
            .writer
            .lock()
            .await
            .write_all(line.as_bytes())
            .await
            .is_err()
        {
            self.poison();
            return false;
        }
        true
    }

    /// 编码成一行（含换行）。`None` = 序列化失败。
    fn encode(payload: serde_json::Value) -> Option<String> {
        let mut line = serde_json::to_string(&Frame::Progress { payload }).ok()?;
        line.push('\n');
        Some(line)
    }

    fn is_poisoned(&self) -> bool {
        self.poisoned.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn poison(&self) {
        self.poisoned
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 请求处理器。
///
/// 做成 trait 而非闭包：处理器要被多个并发连接共享（`Arc<dyn RequestHandler>`），
/// 而闭包形式会把泛型参数一路传染到服务端的每个签名上。
#[async_trait::async_trait]
pub trait RequestHandler: Send + Sync {
    /// `progress` 是处理期间推送非终态消息的出口；不需要就不用它。
    async fn handle(&self, req: Request, progress: &FrameSink) -> Response;
}

/// 服务端：监听通道并应答。
pub struct IpcServer {
    listener: interprocess::local_socket::tokio::Listener,
}

impl IpcServer {
    /// 在给定路径上开始监听。
    ///
    /// 调用前应确保陈旧残留已清理（见 [`super::single`]）——本函数不自行删除既有文件，
    /// 那会让两个进程互相踢掉对方的通道。
    pub fn bind(socket_path: &Path) -> CliResult<Self> {
        let name = socket_name(socket_path)?;
        let listener = ListenerOptions::new()
            .name(name)
            .create_tokio()
            .map_err(|err| CliError::NodeUnavailable(format!("监听本地通道失败: {err}")))?;
        Ok(Self { listener })
    }

    /// 接受一个连接并在**独立任务**里处理它。
    ///
    /// 并发而非串行是必须的：`send` 会阻塞到传输终态（可能是几分钟），串行处理时
    /// 那期间连 `stop` 都递不进来——用户唯一的办法是杀进程。
    ///
    /// 每个连接一问一答后关闭：绝大多数客户端是「连上、问一句、退出」的形态。
    ///
    /// ⚠️ **「一问一答」说的是每条连接上只有一次请求，不是「应答很快」**：
    /// [`Request::Subscribe`] 的应答会持续几天（若干条 [`Frame::Progress`] + 最后一条
    /// 终态）。这里没有超时也没有心跳，也不需要——本地套接字上「对端还在不在」由内核
    /// 直接给出（[`peer_gone`] 读到 EOF），那正是心跳要用一层协议去模拟的东西。
    pub async fn accept_one(&self, handler: std::sync::Arc<dyn RequestHandler>) -> CliResult<()> {
        let stream = self
            .listener
            .accept()
            .await
            .map_err(|err| CliError::NodeUnavailable(format!("接受连接失败: {err}")))?;

        tokio::spawn(async move {
            // 读写分开：处理期间要一边算一边盯着对端有没有走。
            let (read_half, write_half) = tokio::io::split(stream);
            // 写端共享给 [`FrameSink`]：处理期间要往同一条连接推进度，
            // 处理完再往它写终态。**必须是同一个锁**——两边各持一个写端会让
            // 进度帧和终态帧交错成半行，客户端的按行解析当场失败。
            let writer: std::sync::Arc<tokio::sync::Mutex<DynWriter>> =
                std::sync::Arc::new(tokio::sync::Mutex::new(Box::new(write_half)));
            // 进度流被半行污染时终态帧就不写了，理由见 [`FrameSink::send`]。
            let poisoned = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                // 一个字节都没读到就 EOF：探活连接（[`is_alive`]）就长这样——连上、
                // 立刻断开。**不能落到下面去解析空串**，那会为每一次探活凭空造出一条
                // 「无法解析请求」的用法错误，再往一条已经关掉的套接字上写它。
                // `watch` 无节点时每秒探一次，一天就是几万条。
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }

            let response = match serde_json::from_str::<Request>(&line) {
                // **处理期间必须盯着对端**：长轮询类动词会挂十几秒，而客户端完全可能在
                // 这期间被 Ctrl-C 掉。不盯的话这个任务会继续跑到自然结束，把它取走的
                // 东西（确认台的名额、接收锁）一直占着——期间到达的配对请求会被交给一条
                // 已经没人接的连接、就此消失，而下一个客户端还得排在它后面等。
                Ok(req) => {
                    let progress = FrameSink {
                        writer: writer.clone(),
                        poisoned: poisoned.clone(),
                    };
                    tokio::select! {
                        response = handler.handle(req, &progress) => response,
                        _ = peer_gone(&mut reader) => return,
                    }
                }
                // 请求解析失败 = 客户端发来的东西不对，那是用法错误。
                Err(err) => Response::usage(format!("无法解析请求: {err}")),
            };

            let mut out = serde_json::to_string(&Frame::from(response)).unwrap_or_else(|err| {
                // 响应本身序列化失败极罕见，但静默丢弃会让客户端一直等到超时。
                // **必须经 `json!` 而不是手拼**：`err` 里带引号或换行时，手拼出来的是
                // 一行非法 JSON，客户端的解析同样失败——兜底路径于是和它要兜的那个
                // 故障一模一样。`Value` 转字符串不会失败，这里不存在二次兜底的需要。
                serde_json::json!({
                    "kind": "error",
                    // 分类不能漏——少一个字段客户端连这条兜底响应都解析不出来，
                    // 于是它一直等到超时，而超时与「服务端出错」在用户那里是两种表现。
                    "code": Code::NodeUnavailable,
                    "message": format!("响应序列化失败: {err}"),
                })
                .to_string()
            });
            out.push('\n');
            // **流已被污染就别再写**：追加一条合法 JSON 只会拼在半行后面，客户端解析
            // 失败退 3——而那次请求其实是成功的。什么都不写让它读到 EOF，得到一个
            // 诚实的「常驻节点没有应答这条命令」。
            if !poisoned.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = writer.lock().await.write_all(out.as_bytes()).await;
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **进度帧不得被当成答案。**
    ///
    /// 这条通道从一问一答放宽成了「一问、若干条进度、一答」，而绝大多数命令**不认识**
    /// 进度帧。少了这条跳过逻辑，任何一个走通道的命令只要撞上一条进度就会把它当成响应
    /// ——`Data { payload }` 解析成功、内容却是一帧进度，于是命令报告一个空结果并**成功
    /// 退出**。那是最坏的一种失败形态。
    #[test]
    fn a_progress_frame_is_never_terminal() {
        let progress = Frame::Progress {
            payload: serde_json::json!({ "phase": "preparing" }),
        };
        assert!(progress.into_terminal().is_err(), "进度帧被当成了终态");

        for terminal in [
            Frame::Ok,
            Frame::Data {
                payload: serde_json::json!({}),
            },
            Frame::Error {
                code: Code::Usage,
                message: "x".into(),
            },
        ] {
            assert!(terminal.into_terminal().is_ok(), "终态帧被当成了进度");
        }
    }

    /// **失败的分类必须过得了通道。**
    ///
    /// 这条看守的是一个只在「恰好有常驻节点在跑」时才出现的差异：同一件事
    /// （`transfer show <格式合法但不存在的 id>`）在无节点时由本地路径产出
    /// `Usage`（退出码 2），经通道时如果分类丢了，客户端只能一律按「节点不可用」
    /// 处理（退出码 3）。于是 `swarmdrop transfer show $id || retry_if_node_down`
    /// 的行为取决于此刻有没有常驻节点——而 spec「退出码区分失败原因」的整个前提
    /// 是脚本不必解析文本。
    #[test]
    fn error_classification_survives_the_wire() {
        for original in [
            CliError::Usage("没有这条传输记录".into()),
            CliError::PeerUnreachable("拨不通".into()),
            CliError::TransferFailed("中断".into()),
            CliError::PairingRefused("对方拒绝".into()),
            CliError::NodeUnavailable("节点没起来".into()),
        ] {
            let expected = original.code();
            // **必须经 `Frame` 往返**：上线的是它，`Response` 一个字节都不过通道
            // （服务端写 `Frame::from(response)`，客户端解析 `Frame` 再 `into_terminal`）。
            // 只往返 `Response` 的话，给 `Frame::Error` 的 `code` 加一个 `serde(skip)`
            // 这条测试照样绿，而所有经通道的失败在客户端解析失败、一律压成
            // `NodeUnavailable`(3)——正是本测试要挡的那件事。
            let wire = serde_json::to_string(&Frame::from(Response::err(original))).expect("编码");
            let frame: Frame = serde_json::from_str(&wire).expect("往返");
            let back = frame.into_terminal().expect("错误帧是终态");

            let Response::Error { code, .. } = back else {
                panic!("往返后不再是错误响应");
            };
            assert_eq!(
                CliError::from_code(code, String::new()).code(),
                expected,
                "分类在通道上丢了"
            );
        }
    }

    /// **服务端不得产出 `Aborted`。**
    ///
    /// 中止是本地的用户动作（Ctrl-C），通道对面没有立场替用户宣布中止。这条约束支撑着
    /// `CliError::from_code` 里那个丢消息的分支——`CliError::Aborted` 的 Display 是固定的
    /// 「已中止」，服务端若给它配了解释，那句话会静默消失。
    ///
    /// 它同时挡住一类分类错误：传输因「常驻节点被停」而中断时若报 `Aborted`，
    /// 退出码就是 130，而脚本按惯例把 130 读作「人按了 Ctrl-C，别重试」——
    /// 一次本该恢复的中断于是被当成用户主动放弃。那条路径现在报 `TransferFailed`。
    #[test]
    fn aborted_is_never_produced_by_the_server() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cmd/start.rs"))
                .expect("读取通道服务端源码");

        assert!(
            !source.contains("CliError::Aborted"),
            "通道服务端不得产出 Aborted——它的消息会在 from_code 里被丢掉，\
             且退出码 130 会被脚本读作「用户主动放弃」"
        );
    }

    /// 服务端自己发现的用法错误也要带对分类——否则客户端会把它当成节点故障去重试。
    #[test]
    fn server_side_usage_errors_keep_their_code() {
        let Response::Error { code, .. } = Response::usage("不是合法的邀请标识") else {
            panic!("不是错误响应");
        };
        assert_eq!(code, Code::Usage);
    }

    /// 请求与响应都要能往返——通道两端是独立编译的代码路径，形状对不上时
    /// 表现是「命令卡住」而不是编译错误。
    #[test]
    fn protocol_round_trips() {
        let req = Request::Status;
        let text = serde_json::to_string(&req).unwrap();
        assert!(matches!(
            serde_json::from_str::<Request>(&text).unwrap(),
            Request::Status
        ));

        let resp = Response::Data {
            payload: serde_json::json!({"a": 1}),
        };
        let text = serde_json::to_string(&resp).unwrap();
        match serde_json::from_str::<Response>(&text).unwrap() {
            Response::Data { payload } => assert_eq!(payload["a"], 1),
            other => panic!("往返后变了形状: {other:?}"),
        }
    }

    /// 不存在的通道必须判为「没有活节点」，而不是报错。
    #[tokio::test]
    async fn absent_socket_is_not_alive() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_alive(&dir.path().join("nonexistent.sock")).await);
    }

    /// **客户端走了，处理就该停下**，不能继续跑到自然结束。
    ///
    /// 看守的是一个只在长动词上显形的缺陷：处理任务与连接的存活脱钩，于是客户端被
    /// Ctrl-C 掉之后它还占着自己取走的东西（配对确认台的名额、接收锁）直到超时——
    /// 期间到达的配对请求会被交给一条没人接的连接、就此消失，而下一个客户端还得排在
    /// 它后面。这里用一个 `Arc` 探针观察 handler 的 future 有没有被丢弃。
    #[tokio::test]
    async fn handling_stops_when_the_client_walks_away() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct Blocking {
            /// handler 的 future 一旦被丢弃，它持有的这份 clone 就跟着没了。
            canary: Arc<()>,
            finished: Arc<AtomicBool>,
        }

        #[async_trait::async_trait]
        impl RequestHandler for Blocking {
            async fn handle(&self, _req: Request, _progress: &FrameSink) -> Response {
                let _held = self.canary.clone();
                // 挂到天荒地老：真实场景里这是 `desk.take()` 的长轮询。
                std::future::pending::<()>().await;
                self.finished.store(true, Ordering::SeqCst);
                Response::Ok
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.sock");
        let server = IpcServer::bind(&path).unwrap();

        let canary = Arc::new(());
        let finished = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(Blocking {
            canary: canary.clone(),
            finished: finished.clone(),
        });

        let serving = tokio::spawn(async move { server.accept_one(handler).await });

        // 连上、问一句、**立刻走人**（不读应答）。
        {
            let name = socket_name(&path).unwrap();
            let stream = LocalSocketStream::connect(name).await.unwrap();
            let mut writer = BufReader::new(stream);
            writer
                .get_mut()
                .write_all(b"{\"verb\":\"status\"}\n")
                .await
                .unwrap();
            // 作用域结束即断开连接。
        }

        serving.await.unwrap().unwrap();

        // 探针回落到只剩测试自己那一份 ⇒ handler 的 future 已经被丢弃。
        let released = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while Arc::strong_count(&canary) > 1 {
                tokio::task::yield_now().await;
            }
        })
        .await;

        assert!(released.is_ok(), "客户端已断开，处理任务却还占着资源");
        assert!(!finished.load(Ordering::SeqCst), "被放弃的处理不该跑完");
    }

    /// 一问一答的完整往返。
    #[tokio::test]
    async fn server_answers_one_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.sock");

        struct Echo;
        #[async_trait::async_trait]
        impl RequestHandler for Echo {
            async fn handle(&self, req: Request, _progress: &FrameSink) -> Response {
                match req {
                    Request::Status => Response::Data {
                        payload: serde_json::json!({"ok": true}),
                    },
                    _ => Response::Ok,
                }
            }
        }

        let server = IpcServer::bind(&path).unwrap();
        let serving =
            tokio::spawn(async move { server.accept_one(std::sync::Arc::new(Echo)).await });

        let response = request(&path, &Request::Status).await.unwrap();
        match response {
            Some(Response::Data { payload }) => assert_eq!(payload["ok"], true),
            other => panic!("响应不对: {other:?}"),
        }
        serving.await.unwrap().unwrap();
    }

    /// **终态帧的线格式必须与裸 [`Response`] 逐字相同。**
    ///
    /// 这条钉的是跨版本兼容：`Frame` 的三个终态变体是抄 `Response` 的，抄的目的就是让
    /// 旧节点回来的应答新客户端仍然认得（升级 CLI 不会重启常驻节点）。有人「去重」成
    /// `Frame::Terminal { #[serde(flatten)] .. }` 时，JSON 会多一层嵌套，兼容性当场
    /// 消失——而那**不报错**，只在真的遇上版本错位时才显形，那时用户看到的是一句
    /// 「解析响应失败」。
    #[test]
    fn a_terminal_frame_is_byte_identical_to_a_bare_response() {
        for response in [
            Response::Ok,
            Response::Data {
                payload: serde_json::json!({ "sessionId": "x" }),
            },
            Response::Error {
                code: Code::PeerUnreachable,
                message: "拨不通".into(),
            },
        ] {
            let bare = serde_json::to_string(&response).expect("编码 Response");
            let framed = serde_json::to_string(&Frame::from(response)).expect("编码 Frame");
            assert_eq!(framed, bare, "终态帧与裸响应的线格式分叉了");
        }
    }

    /// **「一问、若干条进度、一答」的端到端往返。**
    ///
    /// 这条是这套协议唯一的真看守，它同时钉住三件此前没人钉的事：
    ///
    /// 1. **`request` 会继续读，而不是拿第一帧就返回。** 把它退回单次 `read_line`，
    ///    别的测试全绿，而**所有走通道的命令**在服务端一加进度就全断——拿到的第一帧
    ///    是进度，解析成终态失败。
    /// 2. **进度按序到达，终态在最后。**
    /// 3. **不看进度的老调用方不受影响。** `request` 用的是空回调，服务端新增一条进度
    ///    不该噎住任何一个不认识它的命令。
    ///
    /// 此前这里只有 `Frame::into_terminal` 的单元测试——那是个同义反复（断言一个手写
    /// 四臂 match 的四个臂），它声称要挡的「命令把进度帧当成 `Data` 解析成功」
    /// 根本不可能发生：两者是不同的 serde tag。
    #[tokio::test]
    async fn progress_frames_precede_the_terminal_and_do_not_break_plain_requests() {
        let dir = tempfile::tempdir().unwrap();

        struct Chatty;
        #[async_trait::async_trait]
        impl RequestHandler for Chatty {
            async fn handle(&self, _req: Request, progress: &FrameSink) -> Response {
                for step in 0..3u64 {
                    progress.try_send(serde_json::json!({ "step": step })).await;
                }
                Response::Data {
                    payload: serde_json::json!({ "done": true }),
                }
            }
        }

        // ① 不看进度的调用方拿到的是终态，不是第一帧进度。
        let plain = dir.path().join("plain.sock");
        let server = IpcServer::bind(&plain).unwrap();
        let serving =
            tokio::spawn(async move { server.accept_one(std::sync::Arc::new(Chatty)).await });
        match request(&plain, &Request::Status).await.unwrap() {
            Some(Response::Data { payload }) => assert_eq!(payload["done"], true),
            other => panic!("不看进度的调用方拿到了: {other:?}"),
        }
        serving.await.unwrap().unwrap();

        // ② 看进度的调用方按序收到三条，且终态仍在最后。
        let watching = dir.path().join("watch.sock");
        let server = IpcServer::bind(&watching).unwrap();
        let serving =
            tokio::spawn(async move { server.accept_one(std::sync::Arc::new(Chatty)).await });
        let mut seen = Vec::new();
        let response = request_watching(&watching, &Request::Status, |frame| {
            seen.push(frame["step"].as_u64().expect("进度帧缺 step"));
        })
        .await
        .unwrap();
        assert_eq!(seen, vec![0, 1, 2], "进度帧的条数或顺序不对");
        match response {
            Some(Response::Data { payload }) => assert_eq!(payload["done"], true),
            other => panic!("终态不对: {other:?}"),
        }
        serving.await.unwrap().unwrap();
    }
    /// **一条长驻订阅：很多非终态帧，终态仍在最后。**
    ///
    /// 与上一条不是重复：那条只发三帧，验的是「`request` 会继续读」；这条验的是
    /// 数量级——订阅在一次连接里可能推成千上万条，而客户端那侧的读循环、服务端那侧的
    /// 共享写端与封口标志都要在这个量级上仍然正确。三帧过不了的形状（比如某处只
    /// 处理首帧、或缓冲区在几 KB 之后错位）在三帧下全都是绿的。
    #[tokio::test]
    async fn a_subscription_streams_many_frames_before_its_terminal() {
        const FRAMES: u64 = 1_000;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stream.sock");
        let server = IpcServer::bind(&path).unwrap();

        struct Streamer;
        #[async_trait::async_trait]
        impl RequestHandler for Streamer {
            async fn handle(&self, _req: Request, sink: &FrameSink) -> Response {
                for n in 0..FRAMES {
                    // 订阅走 `send` 而不是 `try_send`：边沿事件不得静默丢失，
                    // 慢就该慢（判据见 [`FrameSink`]）。
                    if !sink.send(serde_json::json!({ "n": n })).await {
                        return Response::Ok;
                    }
                }
                Response::Ok
            }
        }

        let serving =
            tokio::spawn(async move { server.accept_one(std::sync::Arc::new(Streamer)).await });

        let mut seen = Vec::new();
        let response = request_watching(&path, &Request::Status, |frame| {
            seen.push(frame["n"].as_u64().expect("帧缺 n"));
        })
        .await
        .unwrap();

        assert_eq!(seen.len(), FRAMES as usize, "帧数不对");
        assert_eq!(seen.first(), Some(&0));
        assert_eq!(seen.last(), Some(&(FRAMES - 1)), "顺序错了或丢了尾部");
        assert!(
            matches!(response, Some(Response::Ok)),
            "终态不对: {response:?}"
        );
        serving.await.unwrap().unwrap();
    }

    /// **客户端走开之后，服务端那条订阅必须停下来。**
    ///
    /// 订阅没有「处理完了」这个自然终点，唯一的终点就是对端不在了。不停的表现是常驻
    /// 节点里留下一个永远往死连接上写的任务——而这条流的调用方是会反复重启的宿主进程，
    /// 每重启一次留一个。
    ///
    /// 判据是**任务真的结束了**（哨兵析构），不是「计数不再增长」：后者要靠 sleep 猜，
    /// 在忙碌的机器上会假红。
    #[tokio::test]
    async fn a_subscription_stops_when_the_client_walks_away() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.sock");
        let server = IpcServer::bind(&path).unwrap();

        /// 析构即「这条订阅停了」——正常返回与被取消都会走到。
        struct Sentinel(std::sync::Arc<tokio::sync::Notify>);
        impl Drop for Sentinel {
            fn drop(&mut self) {
                self.0.notify_one();
            }
        }

        struct Forever {
            stopped: std::sync::Arc<tokio::sync::Notify>,
        }
        #[async_trait::async_trait]
        impl RequestHandler for Forever {
            async fn handle(&self, _req: Request, sink: &FrameSink) -> Response {
                let _sentinel = Sentinel(self.stopped.clone());
                loop {
                    if !sink.send(serde_json::json!({ "tick": true })).await {
                        return Response::Ok;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }

        let stopped = std::sync::Arc::new(tokio::sync::Notify::new());
        let handler = std::sync::Arc::new(Forever {
            stopped: stopped.clone(),
        });
        let serving = tokio::spawn(async move { server.accept_one(handler).await });

        // 连上、收到至少一帧，然后**走人**（超时取消 → future 析构 → 连接关闭）。
        let mut seen = 0usize;
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            request_watching(&path, &Request::Status, |_| seen += 1),
        )
        .await;
        assert!(seen > 0, "一帧都没收到，这条测试没测到该测的东西");
        serving.await.unwrap().unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(5), stopped.notified())
            .await
            .expect("客户端已经走了，服务端那条订阅还在跑");
    }
}
