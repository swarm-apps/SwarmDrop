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
    /// 解除与某台设备的配对。
    DeviceForget {
        /// 节点标识（完整）。名称到标识的解析在客户端完成。
        peer_id: String,
    },
    /// 生成一张配对邀请。
    ///
    /// **必须由持有节点的那个进程签发**：邀请里带的是签发者的可拨地址，别的进程另起一个
    /// 节点签出来的码指向一个即将消失的临时节点。
    InviteCreate,
    /// 以一个邀请完成配对握手。
    InviteUse {
        invite: String,
    },
    /// 本机已发出且未过期的邀请清单。
    InviteList,
    /// 按 capability 哈希撤销一张邀请。
    ///
    /// 传完整哈希而非用户敲的前缀：前缀要在**当前邀请集合**里解析才谈得上唯一，
    /// 而那个集合客户端已经取过一次了。让服务端再解析一遍等于把同一段逻辑写两份。
    InviteRevoke {
        /// `sha256(capability)` 的小写 hex。
        hash: String,
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
    /// 列出收件箱条目。
    InboxList,
    /// 取一个收件箱条目的详情。
    InboxShow {
        id: String,
    },
    /// 传输记录清单。
    TransferList,
    /// 一条传输记录的详情。
    TransferShow {
        id: String,
    },
    /// 发送文件。**阻塞到传输终态**——客户端期望 `swarmdrop send` 返回时事情已经做完。
    Send {
        /// 源文件/目录的绝对路径。
        paths: Vec<String>,
        /// 目标设备（名称或节点标识）。
        to: String,
    },
    Stop,
}

/// 服务端响应。
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

    let mut buf = String::new();
    reader
        .read_line(&mut buf)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取响应失败: {err}")))?;

    let response = serde_json::from_str(&buf)
        .map_err(|err| CliError::NodeUnavailable(format!("解析响应失败: {err}")))?;
    Ok(Some(response))
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

/// 请求处理器。
///
/// 做成 trait 而非闭包：处理器要被多个并发连接共享（`Arc<dyn RequestHandler>`），
/// 而闭包形式会把泛型参数一路传染到服务端的每个签名上。
#[async_trait::async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle(&self, req: Request) -> Response;
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
    /// 每个连接一问一答后关闭：客户端是「连上、问一句、退出」的形态，
    /// 保持长连接只会多一套超时与心跳逻辑。
    pub async fn accept_one(&self, handler: std::sync::Arc<dyn RequestHandler>) -> CliResult<()> {
        let stream = self
            .listener
            .accept()
            .await
            .map_err(|err| CliError::NodeUnavailable(format!("接受连接失败: {err}")))?;

        tokio::spawn(async move {
            // 读写分开：处理期间要一边算一边盯着对端有没有走。
            let (read_half, mut write_half) = tokio::io::split(stream);
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            if reader.read_line(&mut line).await.is_err() {
                return; // 对端提前断开，不是本端的错误
            }

            let response = match serde_json::from_str::<Request>(&line) {
                // **处理期间必须盯着对端**：长轮询类动词会挂十几秒，而客户端完全可能在
                // 这期间被 Ctrl-C 掉。不盯的话这个任务会继续跑到自然结束，把它取走的
                // 东西（确认台的名额、接收锁）一直占着——期间到达的配对请求会被交给一条
                // 已经没人接的连接、就此消失，而下一个客户端还得排在它后面等。
                Ok(req) => tokio::select! {
                    response = handler.handle(req) => response,
                    _ = peer_gone(&mut reader) => return,
                },
                // 请求解析失败 = 客户端发来的东西不对，那是用法错误。
                Err(err) => Response::usage(format!("无法解析请求: {err}")),
            };

            let mut out = serde_json::to_string(&response).unwrap_or_else(|err| {
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
            let _ = write_half.write_all(out.as_bytes()).await;
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let wire = serde_json::to_string(&Response::err(original)).expect("编码");
            let back: Response = serde_json::from_str(&wire).expect("往返");

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
            async fn handle(&self, _req: Request) -> Response {
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
            async fn handle(&self, req: Request) -> Response {
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
}
