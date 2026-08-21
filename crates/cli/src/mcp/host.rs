//! [`ToolBackend`] 的 CLI 实现。
//!
//! ## 它与三档取数入口的关系
//!
//! MCP server 是**长驻**的，这让它与既有命令有两处不同：
//!
//! 1. **节点持有到 server 退出**，不是一条命令一次。理由见 [`crate::cmd::mcp`]。
//! 2. **取数不能每次现开数据库连接**。`Records` 的连接是惰性且只开一次的，而
//!    `connect_and_migrate` 每次调用都跑一遍 `Migrator::up`——即使无迁移可应用，那也是
//!    一次写事务，会持续在库上开写锁，正好撞上常驻节点写 checkpoint 要的同一把锁
//!    （见 `runtime::access::Records::db` 的警告）。
//!
//! 因此这里持有 [`NodeAccess`] 并复用它的两条路径，而不是自己攒一套：自持节点时直接用
//! 那个节点的 store（零通道往返、零额外连接），复用常驻节点时走本地通道问它。
//!
//! ⚠️ **失败分类必须经 `unpack` 还原**，不能把通道回来的错误一律压成「不可用」：
//! 那会让「没有这条记录」在有常驻节点时和无节点时给出不同的类别。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use swarmdrop_core::transfer::store::TransferStore;

use super::backend::{ToolBackend, ToolError, ToolResult};
use crate::exit::CliError;
use crate::runtime::access::{NodeAccess, to_value, unpack};
use crate::runtime::boot::RunningNode;
use crate::runtime::ipc::Request;
use crate::runtime::transfers::Control;
use crate::runtime::{devices, inbox, transfer, transfers};

impl From<CliError> for ToolError {
    /// 退出码的分类映射到工具的两分类。
    ///
    /// 模型能据以改变行为的只有「我传错了」与「环境不对」两类——它拿到的都是一段要读的
    /// 文字，`PeerUnreachable` 与 `TransferFailed` 的区别对退出码有意义，对它没有。
    fn from(err: CliError) -> Self {
        match err {
            CliError::Usage(msg) => Self::Invalid(msg),
            CliError::NodeUnavailable(msg)
            | CliError::PeerUnreachable(msg)
            | CliError::TransferFailed(msg)
            | CliError::PairingRefused(msg)
            | CliError::UpdateFailed(msg) => Self::Unavailable(msg),
            CliError::Aborted => Self::Unavailable("已中止".into()),
        }
    }
}

pub struct CliToolHost {
    access: NodeAccess,
}

impl CliToolHost {
    pub fn new(access: NodeAccess) -> Self {
        Self { access }
    }

    /// 关停自持节点。复用常驻节点时是空操作——**绝不能顺手把别人的节点关了**。
    ///
    /// ⚠️ **收 `&self` 而不是 `self`**，这不是风格问题。本类型被包进 `Arc` 交给协议栈，
    /// 而协议栈的后台任务可能仍持着一份克隆（尤其在被信号打断、`serve` 那条 future 被
    /// 取消时）。此前的写法是 `Arc::into_inner(host)` 拿所有权，拿不到就 `if let` 静默
    /// 跳过——于是最需要清理的那条路径反而一步都不做，而紧挨着的注释还写着「无论 server
    /// 怎么结束都要走到」。
    ///
    /// 单实例锁不在这里放：它的 `Drop` 会在最后一个持有者散场时跑，而进程退出时操作
    /// 系统也会释放文件锁；残留的套接字文件由下一个拿到锁的进程清理
    /// （`single::acquire` 进门就 `clear_stale_channel`）。
    pub async fn shutdown(&self) {
        self.access.shutdown().await;
    }

    /// 本进程自持的节点。复用常驻节点时没有它，调用方应已先试过通道。
    fn node(&self) -> ToolResult<&RunningNode> {
        self.access
            .local()
            .ok_or_else(|| ToolError::unavailable("节点不可用"))
    }

    fn store(&self) -> ToolResult<Arc<dyn TransferStore>> {
        Ok(self.node()?.manager.transfer_arc().store().clone())
    }

    /// 取一次数据：有常驻节点走通道，自持节点走本地。
    ///
    /// 与 `RecordAccess::query` 同形，但**没有**「通道这一瞬断了就回落本地」那一段——
    /// 那条回落在这里不成立：通道断意味着常驻节点关停，而本进程此时并没有自己的节点
    /// 可回落，`node()` 会诚实地报「节点不可用」。
    async fn query<F, Fut>(&self, request: Request, local: F) -> ToolResult<Value>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ToolResult<Value>>,
    {
        if let Some(payload) = unpack(self.access.ask(&request).await.map_err(ToolError::from)?)
            .map_err(ToolError::from)?
        {
            return Ok(payload);
        }
        local().await
    }
}

#[async_trait]
impl ToolBackend for CliToolHost {
    async fn send_files(&self, paths: Vec<PathBuf>, to: &str) -> ToolResult<Value> {
        let request = Request::Send {
            paths: paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            to: to.to_owned(),
        };
        self.query(request, || async {
            let outcome = transfer::send_files(
                self.node()?,
                &paths,
                to,
                // **什么都不画**：MCP 的 stdout 归协议，而模型要的是最终结果不是过程。
                // `enabled: false` 正是结构化输出模式用的那一档，不另造一个变体。
                transfer::ProgressOut::Bars { enabled: false },
            )
            .await
            .map_err(ToolError::from)?;
            Ok(transfer::file_payload(&outcome))
        })
        .await
    }

    async fn send_text(&self, body: String, to: &str) -> ToolResult<Value> {
        let request = Request::SendText {
            body: body.clone(),
            to: to.to_owned(),
        };
        self.query(request, || async {
            let outcome = transfer::send_text(self.node()?, body, to)
                .await
                .map_err(ToolError::from)?;
            Ok(transfer::text_payload(&outcome))
        })
        .await
    }

    async fn list_devices(&self, online_only: bool) -> ToolResult<Value> {
        let rows = self
            .query(Request::DeviceList, || async {
                let rows = devices::from_node(self.node()?);
                to_value(&rows, "设备清单").map_err(ToolError::from)
            })
            .await?;

        if !online_only {
            return Ok(rows);
        }
        // **在这里筛而不是让通道少送**：`DeviceList` 是共享请求，为一个消费者改它的语义
        // 会让别的调用点跟着变。而「在线」本身是个 `Option`——`None` 是「未知」不是
        // 「离线」（见 `devices::DeviceRow::online`），所以未知的一律不算在线：
        // 模型问「能发给谁」时，把未知说成能发会让它选中一个发不出去的目标。
        Ok(Value::Array(
            crate::runtime::access::rows(rows)
                .into_iter()
                .filter(|row| row.get("online").and_then(Value::as_bool) == Some(true))
                .collect(),
        ))
    }

    async fn list_inbox(&self, limit: Option<u32>, include_archived: bool) -> ToolResult<Value> {
        let rows = self
            .query(Request::InboxList { include_archived }, || async {
                let store = self.store()?;
                let items = inbox::list(&*store, include_archived)
                    .await
                    .map_err(ToolError::from)?;
                to_value(&items, "收件箱").map_err(ToolError::from)
            })
            .await?;

        // **`include_archived` 下推到端口，`limit` 留在这里**，两者的判据不同：
        // 前者改变的是「取哪些行」，在上层按 JSON 筛会静默失效（字段名是 `archived_at`
        // 不是 `archived`）；后者只是截断一份**已经按接收时间倒序**的清单，语义在两侧
        // 完全一致，而端口上压根没有带上限的列表方法。
        //
        // 与 `search_inbox` 的 `limit` 也不冲突：那边的 `None` 有内核给的默认值
        // （`INBOX_SEARCH_LIMIT`，宿主自带一个就会长出第五个答案），这边的 `None`
        // 就是「全都要」，没有可分叉的默认值。
        Ok(truncated(rows, limit))
    }

    async fn search_inbox(
        &self,
        query: &str,
        limit: Option<u32>,
        include_archived: bool,
    ) -> ToolResult<Value> {
        let request = Request::InboxSearch {
            query: query.to_owned(),
            limit,
            include_archived,
        };
        self.query(request, || async {
            let store = self.store()?;
            let hits = inbox::search(&*store, query, limit, include_archived)
                .await
                .map_err(ToolError::from)?;
            to_value(&hits, "检索结果").map_err(ToolError::from)
        })
        .await
    }

    async fn inbox_item(&self, item_id: &str) -> ToolResult<Value> {
        // 格式错误在本地先判一次，好让它在没有常驻节点时也立刻是用法错误而不是通道往返。
        inbox::parse_id(item_id).map_err(ToolError::from)?;
        let request = Request::InboxShow {
            id: item_id.to_owned(),
        };
        self.query(request, || async {
            let store = self.store()?;
            let detail = inbox::detail(&*store, item_id)
                .await
                .map_err(ToolError::from)?;
            to_value(&detail, "条目详情").map_err(ToolError::from)
        })
        .await
    }

    async fn inbox_file_path(&self, item_id: &str, relative_path: &str) -> ToolResult<PathBuf> {
        let detail = self.inbox_item(item_id).await?;

        let file =
            crate::runtime::access::rows(detail.get("files").cloned().unwrap_or(Value::Null))
                .into_iter()
                .find(|f| f.get("relativePath").and_then(Value::as_str) == Some(relative_path))
                .ok_or_else(|| {
                    ToolError::invalid(format!("条目 {item_id} 里没有文件 {relative_path}"))
                })?;

        let path = file
            .get("localPath")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .ok_or_else(|| {
                ToolError::invalid(format!(
                    "{relative_path} 没有可用的本地路径，它可能已被移走"
                ))
            })?;

        // **落地检查**：spec 明写不得返回无效路径。模型拿到无效路径后会把它传给别的
        // 工具，失败会出现在离原因很远的地方。
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(ToolError::invalid(format!(
                "{relative_path} 的记录指向 {}，但那里已经没有文件了",
                path.display()
            )));
        }
        Ok(path)
    }

    async fn list_transfers(&self, limit: Option<u32>) -> ToolResult<Value> {
        let rows = self
            .query(Request::TransferList, || async {
                let store = self.store()?;
                let rows = transfers::list(&*store).await.map_err(ToolError::from)?;
                to_value(&rows, "传输记录").map_err(ToolError::from)
            })
            .await?;

        Ok(truncated(rows, limit))
    }

    async fn transfer_status(&self, session_id: &str) -> ToolResult<Value> {
        transfers::parse_id(session_id).map_err(ToolError::from)?;
        let request = Request::TransferShow {
            id: session_id.to_owned(),
        };
        self.query(request, || async {
            let store = self.store()?;
            let row = transfers::show(&*store, session_id)
                .await
                .map_err(ToolError::from)?;
            to_value(&row, "传输详情").map_err(ToolError::from)
        })
        .await
    }

    async fn control_transfer(&self, session_id: &str, action: Control) -> ToolResult<Value> {
        let request = Request::TransferControl {
            action,
            ids: vec![session_id.to_owned()],
        };
        self.query(request, || async {
            let outcome = transfers::control(
                self.node()?.manager.transfer(),
                action,
                &[session_id.to_owned()],
            )
            .await
            .map_err(ToolError::from)?;
            to_value(&outcome, "运行控制结果").map_err(ToolError::from)
        })
        .await
    }

    async fn network_status(&self) -> ToolResult<Value> {
        self.query(Request::Status, || async {
            let status = self.node()?.manager.get_network_status();
            to_value(&status, "节点状态").map_err(ToolError::from)
        })
        .await
    }
}

/// 把一份清单按调用方给的上限截断。
///
/// 两个清单工具（收件箱、传输记录）都要这一步，而它们的语义前提相同：两份清单都由端口
/// 保证按时间倒序，所以「截断」等于「取最近的那几条」，与在端口那侧截断没有区别。
///
/// `None` = 全都要。**这里刻意没有默认值**——有默认值的那个是 `search_inbox`，它的默认
/// 归内核（`INBOX_SEARCH_LIMIT`），宿主自带一份就会长出第五个答案（见
/// [`crate::runtime::inbox::search`]）。
fn truncated(rows: Value, limit: Option<u32>) -> Value {
    let mut rows = crate::runtime::access::rows(rows);
    if let Some(limit) = limit {
        rows.truncate(limit as usize);
    }
    Value::Array(rows)
}
