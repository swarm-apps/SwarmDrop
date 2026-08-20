//! 传输记录的查询。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）。
//!
//! 记录全在本机的库里，读它不需要网络——所以两条路径都不起节点，有常驻节点时走通道
//! 只是为了避开 SQLite 的写锁（判据见 [`super::access`]）。
//!
//! 函数收 **`&dyn TransferStore` 而不是 `Records`**：常驻节点那侧的 store 已经握在
//! `TransferManager` 手里，收 `Records` 会逼它另开一个数据库连接读同一份数据——于是
//! 通道服务端只能把这几行逻辑再抄一遍（连错误措辞一起）。收端口就两条路径共用一份。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use swarmdrop_core::transfer::manager::TransferManager;
use swarmdrop_core::transfer::store::{TransferProjection, TransferStore};
use uuid::Uuid;

use crate::exit::{CliError, CliResult};

/// 全部传输记录。
///
/// **不在本层重排序**：端口契约已保证按 `started_at` 倒序，而那条契约的存在理由正是
/// 「同一份数据两次调用必须给出同一序」。再排一次只会掩盖端口实现违约的情形。
pub async fn list(store: &dyn TransferStore) -> CliResult<Vec<TransferProjection>> {
    store
        .list_transfer_projections()
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取传输记录失败: {err}")))
}

/// 未完成的传输记录（`phase != Terminal`）。
///
/// **不是 [`list`] 加一行 `filter`**：这份清单是 `transfer watch` 每秒重取一次的东西，
/// 而传输历史只增不减。过滤下推到存储端口之后，读的行数只随「此刻真的没传完的那几条」
/// 变化，而不是随这台机器用了多久变化。
pub async fn unfinished(store: &dyn TransferStore) -> CliResult<Vec<TransferProjection>> {
    store
        .list_unfinished_projections()
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取传输记录失败: {err}")))
}

/// 一条传输记录。
pub async fn show(store: &dyn TransferStore, id: &str) -> CliResult<TransferProjection> {
    let uuid = parse_id(id)?;
    store
        .get_transfer_projection(uuid)
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("读取传输记录失败: {err}")))?
        .ok_or_else(|| CliError::Usage(format!("没有这条传输记录: {id}")))
}

/// 会话标识解析。
///
/// **格式错误是用法错误**，不是「找不到」：前者要用户改参数，后者要用户换一条记录，
/// 而退出码要能区分它们。
pub fn parse_id(id: &str) -> CliResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| CliError::Usage(format!("不是合法的会话标识: {id}")))
}

/// 传输阶段在通道上的名字（`entity::TransferPhase` 的 serde 形态）。
///
/// 抄一份而不是依赖 `entity`：本 crate 的生产代码只认端口与 JSON——两条取数路径
/// （直连本机记录 / 经通道问常驻节点）里只有前者拿得到 typed 值，判据要对两者都成立。
///
/// **抄这一份，别抄第二份**：判据（能不能暂停）、文案（`render::transfer::phase_label`）
/// 与面板样式（`render::watch`）都要认这些名字，各写各的字面量就等于把下面那条风险
/// 复制三遍，而只有一份被测到。
///
/// ⚠️ 抄来的字符串会**静默**漂移：核心改了 `rename_all` 或变体名，这里的判据会全部
/// 落空——`transfer pause` 于是报「没有正在传输的会话」，而屏幕上明明有一条在传。
/// `phase_names_match_the_wire` 是唯一的看守。
pub const PHASE_OFFERED: &str = "offered";
pub const PHASE_WAITING_ACCEPT: &str = "waiting_accept";
pub const PHASE_ACTIVE: &str = "active";
pub const PHASE_SUSPENDED: &str = "suspended";
pub const PHASE_TERMINAL: &str = "terminal";

/// 对一条未完成的传输可以做的运行控制。
///
/// 三个动作共用一个枚举、在通道上也只占一个动词，而不是三个：它们的骨架完全一致
/// （解析标识 → 按方向派生 → 汇总结果），拆开只会让同一段代码在服务端出现三遍。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Pause,
    Resume,
    Cancel,
}

impl Control {
    /// 这个动作此刻对这条会话成立吗——**判据只有这一份**。
    ///
    /// 它同时是三处的依据：菜单里列哪些候选、参数指定的那条认不认、`watch` 的热键
    /// 提示行显示什么。分开写的话，用户会在菜单里选到一条随即被服务端拒绝的会话。
    ///
    /// 规则与桌面端 UI 的按钮可用性一致（`src/lib/transfer-projection.ts` +
    /// `-session-row.tsx`）。三端同一件事给出不同的可用集，比给出错误的结果更难排查
    /// ——用户会以为是这台设备坏了。
    pub fn applies(self, row: &Value) -> bool {
        let phase = row.get("phase").and_then(Value::as_str);
        match self {
            // 暂停要有一个正在跑的 actor。`offered` / `waiting_accept` 阶段还没有，
            // 那时该做的是取消（下面那条覆盖了它们）。
            Self::Pause => phase == Some(PHASE_ACTIVE),
            // 恢复要断点信息完好。不可恢复的中断只能重新发一次，`resume` 对它无能为力。
            Self::Resume => {
                phase == Some(PHASE_SUSPENDED)
                    && row.get("recoverable").and_then(Value::as_bool) == Some(true)
            }
            // 取消覆盖「已经开始、尚未结束」的全部三个阶段。
            // 已暂停/已中断的不在其列：那时没有活 actor 可取消，能做的是删记录。
            Self::Cancel => matches!(
                phase,
                Some(PHASE_OFFERED | PHASE_WAITING_ACCEPT | PHASE_ACTIVE)
            ),
        }
    }
}

/// 一次运行控制的结局。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlOutcome {
    pub action: Control,
    /// 做成了的会话标识。
    pub done: Vec<String>,
    /// 没做成的，连同原因。
    ///
    /// **必须如实列出而不是汇总成一个失败**：一次勾选三条、成了两条时，用户要知道
    /// 是哪一条没成。全部压成 `Err` 则连成了的那两条也看不见了。
    pub failed: Vec<ControlFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlFailure {
    pub id: String,
    pub reason: String,
}

/// 对若干条会话执行同一个运行控制。
///
/// **先把标识全部解析完再动手**：其中一个格式不合法时一条都不该执行——用户敲错时的
/// 正确处置是停下来让他看清楚，而不是做掉另外几条之后再说有一个没认出来。这与
/// [`super::invites::revoke_each`] 是同一条规则。
///
/// 解析之后**不再短路**：某一条失败（actor 恰好在这一瞬结束了）不该让排在它后面的
/// 都不执行。
pub async fn control(
    transfer: &TransferManager,
    action: Control,
    ids: &[String],
) -> CliResult<ControlOutcome> {
    let parsed: Vec<Uuid> = ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<CliResult<_>>()?;

    let mut outcome = ControlOutcome {
        action,
        done: Vec::with_capacity(parsed.len()),
        failed: Vec::new(),
    };
    for session_id in parsed {
        match apply(transfer, action, session_id).await {
            Ok(()) => outcome.done.push(session_id.to_string()),
            Err(reason) => outcome.failed.push(ControlFailure {
                id: session_id.to_string(),
                reason,
            }),
        }
    }
    Ok(outcome)
}

/// 执行一条。
///
/// 三个动作都走**方向自派生**的域入口（`pause` / `cancel` / `initiate_resume`），
/// 本层不碰 `direction`：那需要 `entity` 的枚举，而命令行宿主的生产代码只认端口与 JSON。
async fn apply(
    transfer: &TransferManager,
    action: Control,
    session_id: Uuid,
) -> Result<(), String> {
    match action {
        Control::Pause => transfer.pause(&session_id).await.map_err(|e| e.to_string()),
        Control::Cancel => transfer
            .cancel(&session_id)
            .await
            .map_err(|e| e.to_string()),
        // 恢复返回的是新一轮的会话信息，命令面用不上——它要的只是「续上了没有」，
        // 之后的进度由 `watch` 或事件呈现。
        Control::Resume => transfer
            .initiate_resume(session_id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_id_is_a_usage_error() {
        let err = parse_id("not-a-uuid").expect_err("应当拒绝");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    #[test]
    fn well_formed_id_parses() {
        let id = Uuid::new_v4();
        assert_eq!(parse_id(&id.to_string()).expect("解析"), id);
    }

    fn row(phase: &str, recoverable: bool) -> Value {
        serde_json::json!({ "phase": phase, "recoverable": recoverable })
    }

    /// **本模块认识的 phase 名必须与核心实际序列化出来的一致。**
    ///
    /// 这里的四个常量是抄来的（生产代码不依赖 `entity`），而抄来的字符串会**静默**漂移：
    /// 核心改了 `rename_all` 或变体名之后，[`Control::applies`] 会全部落空——
    /// `transfer pause` 报「没有正在传输的会话」，而屏幕上明明有一条在传，
    /// 不报错、不 panic、别的测试也不红。除非有这一条。
    #[test]
    fn phase_names_match_the_wire() {
        for (variant, expected) in [
            (entity::TransferPhase::Offered, PHASE_OFFERED),
            (entity::TransferPhase::WaitingAccept, PHASE_WAITING_ACCEPT),
            (entity::TransferPhase::Active, PHASE_ACTIVE),
            (entity::TransferPhase::Suspended, PHASE_SUSPENDED),
        ] {
            let wire = serde_json::to_value(&variant).expect("序列化");
            assert_eq!(wire.as_str(), Some(expected), "{variant:?} 的 wire 名变了");
        }
    }

    /// 暂停只对**正在传**的会话成立——其余阶段没有活 actor 可暂停。
    #[test]
    fn pause_only_applies_to_active_sessions() {
        assert!(Control::Pause.applies(&row(PHASE_ACTIVE, false)));
        for phase in [PHASE_OFFERED, PHASE_WAITING_ACCEPT, PHASE_SUSPENDED] {
            assert!(
                !Control::Pause.applies(&row(phase, true)),
                "{phase} 不该能暂停"
            );
        }
    }

    /// **不可恢复的中断不得进恢复菜单。**
    ///
    /// 它看起来与「已暂停」在同一个 phase 上，但断点信息已经没了——列进去的结果是
    /// 用户选中它、命令报一个来自域深处的错，而他本该被告知「这条只能重发」。
    #[test]
    fn resume_needs_a_recoverable_suspension() {
        assert!(Control::Resume.applies(&row(PHASE_SUSPENDED, true)));
        assert!(!Control::Resume.applies(&row(PHASE_SUSPENDED, false)));
        assert!(!Control::Resume.applies(&row(PHASE_ACTIVE, true)));
    }

    /// 取消覆盖「开始了但没结束」的三个阶段，且**不含已暂停**——那时没有活 actor，
    /// 用户该做的是删记录。
    #[test]
    fn cancel_covers_every_live_phase() {
        for phase in [PHASE_OFFERED, PHASE_WAITING_ACCEPT, PHASE_ACTIVE] {
            assert!(
                Control::Cancel.applies(&row(phase, false)),
                "{phase} 该能取消"
            );
        }
        assert!(!Control::Cancel.applies(&row(PHASE_SUSPENDED, true)));
    }

    /// 缺字段一律不成立——旧版本的记录、或通道对面回了个非预期形状时，
    /// **宁可少列一条候选，也不能把一个做不成的动作摆到用户面前**。
    #[test]
    fn a_row_without_a_phase_offers_nothing() {
        let bare = serde_json::json!({});
        for action in [Control::Pause, Control::Resume, Control::Cancel] {
            assert!(!action.applies(&bare), "{action:?} 不该对无 phase 的行成立");
        }
    }

    /// 动作名要能过通道——两端是独立编译的代码路径，形状对不上时表现是「命令卡住」。
    #[test]
    fn control_round_trips() {
        for action in [Control::Pause, Control::Resume, Control::Cancel] {
            let wire = serde_json::to_string(&action).expect("编码");
            let back: Control = serde_json::from_str(&wire).expect("往返");
            assert_eq!(back, action);
        }
    }
}
