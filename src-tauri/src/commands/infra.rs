//! 引导节点（基础设施关系）的意图 IPC。
//!
//! # 为什么校验规则不在 TS 里
//!
//! 「这条地址本端拨得动吗」的判据是**内核事实**（`Endpoint::supported_transports()`
//! 与合法 peer id 形状），不是部署配置。前端复制一份必然漂移——桌面那份会忘掉浏览器
//! 没有 TCP，浏览器那份会忘掉 native 没有打洞地址。所以三条命令全部只做转发，
//! 判据住在 [`swarmdrop_core::infra`]。
//!
//! # 为什么返回 `Option<InfraAddrError>` 而不是 `Result<_, InfraAddrError>`
//!
//! 「用户粘了一条不合法的地址」是输入框的**正常结果**，不是异常；而「节点还没启动」
//! 才是这条调用根本问不出答案的故障。两者分层：前者是返回值（`None` = 通过），
//! 后者走 [`AppError`](crate::AppError) 抛出。合成一个 `Result` 会逼前端用 try/catch
//! 处理每一次按键校验，还得在 catch 里区分这两类东西。
//!
//! 被拒原因**原样下发**（`InfraAddrError` 自带 `kind` 判别码），前端按变体渲染，
//! 不许去匹配错误字符串。

use swarmdrop_core::infra::{InfraAddrError, supported_transport_names};
use swarmdrop_core::network::candidates::CandidateRoles;
use swarmdrop_net::NodeId;
use tauri::State;

use crate::AppError;
use crate::network::NetManagerState;

/// 校验一条用户输入的引导节点地址，**不写任何状态**。
///
/// 供输入框拿内联错误用。全部规则零网络往返——「能不能连上」由提交后的收敛环回答。
#[tauri::command]
#[specta::specta]
pub async fn validate_infra_addr(
    net: State<'_, NetManagerState>,
    addr: String,
) -> crate::AppResult<Option<InfraAddrError>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or_else(AppError::node_not_started)?;
    Ok(manager.validate_infra_addr(&addr).err())
}

/// 添加一个引导节点：校验 + 登记，**一次调用内原子完成**。
///
/// 拆成「先 validate 再 ensure」会在两次 IPC 往返之间留一个窗口，期间候选表可能已变。
///
/// 登记成功后**无需重启节点**：收敛环最迟下一个 1s tick 起第一轮，状态经
/// `networkStatusChanged` 推回来。
#[tauri::command]
#[specta::specta]
pub async fn add_infra_node(
    net: State<'_, NetManagerState>,
    addr: String,
) -> crate::AppResult<Option<InfraAddrError>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or_else(AppError::node_not_started)?;
    // 角色给全：手动配的引导节点既当 DHT 种子也当中继候选。真正是否建 reservation
    // 由 supervisor 按 `public_reachability` 闸门决定，不在这里预判。
    Ok(manager
        .add_infra_node(&addr, CandidateRoles::kad_and_relay())
        .err())
}

/// 撤销一个引导节点的常驻意图。
///
/// **会立刻断开与该节点的全部连接**（含中止在途拨号），所以调用点必须按
/// `InfraLink.removable` 门控——自动发现来源的节点可能同时是一台正在传文件的已配对设备。
#[tauri::command]
#[specta::specta]
pub async fn remove_infra_node(
    net: State<'_, NetManagerState>,
    peer_id: String,
) -> crate::AppResult<()> {
    let node: NodeId = peer_id
        .parse()
        .map_err(|e| AppError::invalid_argument(format!("非法的节点 ID: {e}")))?;
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or_else(AppError::node_not_started)?;
    manager.remove_infra_intent(node).await?;
    Ok(())
}

/// 本端点实际装配的传输 wire 名清单（`tcp` / `quic` / `webrtc` / `webrtcDirect`）。
///
/// 给 UI 写「本端支持 …」用。它是**运行时事实**而非编译期常量：打洞传输按配置装配，
/// 所以前端不能照着平台猜。
#[tauri::command]
#[specta::specta]
pub async fn supported_transports(
    net: State<'_, NetManagerState>,
) -> crate::AppResult<Vec<String>> {
    let guard = net.lock().await;
    let manager = guard.as_ref().ok_or_else(AppError::node_not_started)?;
    Ok(supported_transport_names(manager.endpoint())
        .into_iter()
        .collect())
}
