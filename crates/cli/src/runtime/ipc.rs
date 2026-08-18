//! 本地通道：其余命令经它复用正在运行的节点。
//!
//! **这是内部机制，不是对外 API**。两端都是本 crate 的代码，动词集就是命令面的映射，
//! 可以随时改。若将来要把能力暴露给外部程序，那时再基于真实需求决定是提升这套通道还是
//! 另起一个面——现在为一个未定的消费者做通用化是投机。
//!
//! 传输走本地套接字（类 Unix 是域套接字、Windows 是命名管道），载荷是**行分隔的 JSON**：
//! 一行一条消息。选它不是因为高效，是因为出问题时可以直接用 `nc` 看——一个内部调试通道
//! 的可读性比它的字节数重要。

use std::path::Path;

use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::exit::{CliError, CliResult};

/// 客户端请求。
///
/// 动词与命令面一一对应——多一个命令就多一个变体，不做通用的「转发任意调用」，
/// 那会把这层变成一个需要版本协商的 API。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum Request {
    Status,
    Devices,
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
    Error { message: String },
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
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).await.is_err() {
                return; // 对端提前断开，不是本端的错误
            }

            let response = match serde_json::from_str::<Request>(&line) {
                Ok(req) => handler.handle(req).await,
                Err(err) => Response::Error {
                    message: format!("无法解析请求: {err}"),
                },
            };

            let mut out = serde_json::to_string(&response).unwrap_or_else(|err| {
                // 响应本身序列化失败极罕见，但静默丢弃会让客户端一直等到超时。
                format!(r#"{{"kind":"error","message":"响应序列化失败: {err}"}}"#)
            });
            out.push('\n');
            let _ = reader.get_mut().write_all(out.as_bytes()).await;
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
