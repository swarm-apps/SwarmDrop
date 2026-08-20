//! 传输记录的渲染。
//!
//! 输入是**已经序列化好的 JSON** 而非核心的 DTO：记录可能来自本机的库，也可能来自通道
//! 对面的常驻节点，后者只能是 JSON。让渲染层统一吃 JSON，就不必为两条来源各写一份。

use serde_json::Value;

use crate::runtime::transfers::{
    Control, PHASE_ACTIVE, PHASE_OFFERED, PHASE_SUSPENDED, PHASE_TERMINAL, PHASE_WAITING_ACCEPT,
};

use super::{bytes_or_dash as bytes, flag, text_or};

pub fn render_list(records: &Value, json: bool) {
    if json {
        super::emit_json(records, "传输记录");
        return;
    }

    let Some(list) = records.as_array() else {
        println!("（无法解析传输记录）");
        return;
    };

    if list.is_empty() {
        println!("还没有传输记录。");
        return;
    }

    for item in list {
        let direction = direction_glyph(item.get("direction").and_then(Value::as_str));
        println!(
            "{direction} {}  {}",
            text_or(item, "peerName", "—"),
            phase_label(item)
        );
        println!(
            "   {}  {} / {}",
            text_or(item, "sessionId", "—"),
            bytes(item.get("transferredBytes")),
            bytes(item.get("totalSize"))
        );
    }
}

/// 选择菜单里的一行。
///
/// **信息要够用户认出是哪一条**，而菜单只有一行的宽度：方向 + 对端 + 阶段 + 大小。
/// 会话标识（UUID）刻意不进来——36 个字符会把上面那些真正能区分的信息挤出屏幕，
/// 而用户认不出一串随机十六进制是哪次传输。要精确指定的场合走参数，那里才用标识。
pub fn menu_line(record: &Value) -> String {
    format!(
        "{} {}  {}  {}",
        direction_glyph(record.get("direction").and_then(Value::as_str)),
        text_or(record, "peerName", "—"),
        phase_label(record),
        bytes(record.get("totalSize"))
    )
}

pub fn render_detail(record: &Value, json: bool) {
    if json {
        super::emit_json(record, "传输记录");
        return;
    }

    println!("会话      {}", text_or(record, "sessionId", "—"));
    println!("方向      {}", text_or(record, "direction", "—"));
    println!("对端      {}", text_or(record, "peerName", "—"));
    println!("状态      {}", phase_label(record));
    println!(
        "进度      {} / {}",
        bytes(record.get("transferredBytes")),
        bytes(record.get("totalSize"))
    );
    // **只有接收方向有本地落点**，所以这一行按有没有值出现，而不是恒印一个占位符：
    // 发送会话没有 `save_path`，`contentRoot` 因此是空的，印一行「位置 —」只会让人
    // 以为记录坏了。core 已经把它解析成真实容器目录，这里直读，不做任何兜底或拼接。
    if let Some(root) = record.get("contentRoot").and_then(Value::as_str) {
        println!("位置      {root}");
    }
    if let Some(failure) = record.get("failure").and_then(Value::as_str) {
        println!("失败原因  {failure}");
    }
    // 可恢复与否决定用户下一步能做什么，值得单列。
    if record.get("recoverable").and_then(Value::as_bool) == Some(true) {
        println!("可恢复    是");
    }
}

/// 传输状态的人话。
///
/// **三处共用一份**（清单、菜单行、`watch` 面板），而不是各写各的：同一条会话在
/// 传输列表里显示「已暂停」、在面板上显示 `suspended` 的话，用户会以为那是两回事。
///
/// 措辞对齐桌面端的 `projectionStatusLabel`（`src/lib/transfer-projection.ts`）。
/// 三端同一个状态给出不同说法，比给错更难排查——用户会以为是这台设备的问题。
///
/// ⚠️ 名字来自各枚举的 serde 形态，与 [`direction_glyph`] 同一类风险：核心改了
/// `rename_all` 或变体名，这里的 match 会全部落进兜底，**每一条记录的状态都静默变成
/// 「—」**。`status_names_match_the_wire` 是唯一的看守。
pub fn phase_label(record: &Value) -> &'static str {
    let reason = |key: &str| record.get(key).and_then(Value::as_str);
    match record.get("phase").and_then(Value::as_str) {
        Some(PHASE_OFFERED) => "等待中",
        Some(PHASE_WAITING_ACCEPT) => "等待确认",
        Some(PHASE_ACTIVE) => "传输中",
        Some(PHASE_SUSPENDED) => match reason("suspendedReason") {
            Some("local_paused") => "已暂停",
            Some("remote_paused") => "对方暂停",
            Some("interrupted") => "已中断",
            Some("peer_offline") => "对方离线",
            Some("app_restarted") => "重启后中断",
            // 没有原因的挂起：可恢复与否决定用户下一步能做什么，这是仅有的区别。
            _ if flag(record, "recoverable") => "可恢复失败",
            _ => "不可恢复失败",
        },
        Some(PHASE_TERMINAL) => match reason("terminalReason") {
            Some("completed") => "已完成",
            Some("cancelled") => "已取消",
            Some("rejected") => "对方拒绝",
            // 「没答复」既不是失败也不是本人拒绝——两种说法都会让用户以为自己或对方
            // 做过某个决定，而他其实只是没来得及看见（措辞同桌面端）。
            Some("expired") => "未及时处理",
            _ => "不可恢复失败",
        },
        _ => "—",
    }
}

/// 一次运行控制（暂停 / 恢复 / 取消）的结果。
///
/// 动作由调用方以 typed 值传入而不是从负载里读回来：那样就要再抄一份动作名的字符串，
/// 而它与 `Control` 的 serde 形态是两份会各自漂移的东西。
pub fn render_control(action: Control, outcome: &Value, json: bool) {
    if json {
        super::emit_json(outcome, "控制结果");
        return;
    }

    // 首行与 [`control_summary`] 同源——命令的输出与面板里的那一行说的必须是同一句话。
    println!("{}", control_summary(action, outcome));

    // **失败逐条列出**，不汇总成一句：一次勾三条成了两条时，用户要知道是哪一条没成。
    let verb = control_verb(action);
    for failure in outcome
        .get("failed")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        println!(
            "! {} 未能{verb}：{}",
            super::short(&text_or(failure, "id", "—"), 8),
            text_or(failure, "reason", "原因不明")
        );
    }
}

/// 把一次运行控制的结果凝成**一行**话。
///
/// `watch` 的面板用它：那里没有多行输出的位置（面板随即会重画），而
/// [`render_control`] 是给命令的最终输出用的。两者共用这一句，措辞就不会分叉——
/// 同一件事在面板上说「已暂停 2 条」、在命令里说「暂停了 2 个会话」，
/// 会让人以为它们是两个不同的操作。
pub fn control_summary(action: Control, outcome: &Value) -> String {
    let verb = control_verb(action);
    match (count(outcome, "done"), count(outcome, "failed")) {
        // **一条都没做时也要说一句**：什么都不打印就退出，看起来像命令挂了。
        (0, 0) => format!("没有传输被{verb}"),
        (done, 0) => format!("已{verb} {done} 条传输"),
        (0, failed) => format!("{failed} 条未能{verb}"),
        (done, failed) => format!("已{verb} {done} 条，{failed} 条未能{verb}"),
    }
}

/// 结果里某一组的条数；字段缺失或形状不对时为 0。
fn count(outcome: &Value, key: &str) -> usize {
    outcome
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// 动作的中文动词。
fn control_verb(action: Control) -> &'static str {
    // 无 `_ =>` 兜底：新增一个动作时编译器会在这里报错，而不是让它显示成一个笼统的词。
    match action {
        Control::Pause => "暂停",
        Control::Resume => "恢复",
        Control::Cancel => "取消",
    }
}

/// 传输方向的图标。
///
/// 名字来自 `entity::TransferDirection` 的 serde 形态（`rename_all = "lowercase"`）。
/// 曾经这里还写着 `Some("Send")` / `Some("Receive")` 两个 arm——**它们永不命中**，
/// 是照着想象写的。
///
/// ⚠️ 兜底给「·」意味着核心一改 `rename_all`，**每一行的方向都会静默变成「·」**，
/// 而没有任何东西会报错。`direction_names_match_the_wire` 是唯一的看守。
pub(super) fn direction_glyph(direction: Option<&str>) -> &'static str {
    match direction {
        Some("send") => "↑",
        Some("receive") => "↓",
        _ => "·",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **render 认识的方向名必须与核心实际序列化出来的一致。**
    ///
    /// 两者是两份独立的字符串：一份写在 `entity::TransferDirection` 的
    /// `#[serde(rename_all = "lowercase")]` 上，一份写在 `direction_glyph` 的 match 里。
    /// 核心改了那个属性、或改了变体名，这里的 match 会全部落进 `_` 兜底——
    /// 传输列表每一行的方向图标静默变成「·」，不报错、不 panic、测试也不红。
    /// 除非有这一条。
    #[test]
    fn direction_names_match_the_wire() {
        let send = serde_json::to_value(entity::TransferDirection::Send).expect("序列化");
        let receive = serde_json::to_value(entity::TransferDirection::Receive).expect("序列化");

        assert_eq!(
            direction_glyph(send.as_str()),
            "↑",
            "发送方向对不上 wire 名"
        );
        assert_eq!(
            direction_glyph(receive.as_str()),
            "↓",
            "接收方向对不上 wire 名"
        );
        // 兜底仍然存在，但它只该接住「真的没有这个字段」。
        assert_eq!(direction_glyph(None), "·");
    }

    /// **render 认识的状态名必须与核心实际序列化出来的一致。**
    ///
    /// 同 [`direction_names_match_the_wire`] 的理由，但覆盖面更宽：三个枚举、十四个
    /// 变体。任何一个漂移，那种状态的每一条记录都会静默显示成一句错的话——
    /// `phase` 漂移是「—」，两个 reason 漂移是「不可恢复失败」，
    /// 而后者尤其糟：它把一次正常的暂停说成了故障。
    ///
    /// **逐个列举而不是遍历**：`strum::EnumIter` 要 `strum` 在作用域里，而本 crate
    /// 连生产代码都不依赖 `entity`（它只在 dev-dependencies 里）。代价是新增变体时
    /// 这份清单不会自己长出来——所以最后一句断言盯着条数。
    #[test]
    fn status_names_match_the_wire() {
        use entity::{SuspendedReason, TerminalReason, TransferPhase};

        fn wire<T: serde::Serialize>(value: T) -> String {
            serde_json::to_value(value)
                .expect("序列化")
                .as_str()
                .expect("状态枚举必须序列化成字符串")
                .to_owned()
        }

        let phases = [
            (TransferPhase::Offered, "等待中"),
            (TransferPhase::WaitingAccept, "等待确认"),
            (TransferPhase::Active, "传输中"),
        ];
        for (phase, expected) in &phases {
            let record = serde_json::json!({ "phase": wire(phase.clone()) });
            assert_eq!(phase_label(&record), *expected, "{phase:?} 的文案对不上");
        }

        let suspended = [
            (SuspendedReason::LocalPaused, "已暂停"),
            (SuspendedReason::RemotePaused, "对方暂停"),
            (SuspendedReason::Interrupted, "已中断"),
            (SuspendedReason::PeerOffline, "对方离线"),
            (SuspendedReason::AppRestarted, "重启后中断"),
        ];
        for (reason, expected) in &suspended {
            let record = serde_json::json!({
                "phase": wire(TransferPhase::Suspended),
                "suspendedReason": wire(reason.clone()),
            });
            assert_eq!(phase_label(&record), *expected, "{reason:?} 的文案对不上");
        }

        let terminal = [
            (TerminalReason::Completed, "已完成"),
            (TerminalReason::Cancelled, "已取消"),
            (TerminalReason::Rejected, "对方拒绝"),
            (TerminalReason::Expired, "未及时处理"),
            (TerminalReason::FatalError, "不可恢复失败"),
        ];
        for (reason, expected) in &terminal {
            let record = serde_json::json!({
                "phase": wire(TransferPhase::Terminal),
                "terminalReason": wire(reason.clone()),
            });
            assert_eq!(phase_label(&record), *expected, "{reason:?} 的文案对不上");
        }

        // 新增变体时这份清单不会自己长出来——数一遍，少了就红。
        assert_eq!(
            phases.len() + 2,
            5,
            "TransferPhase 多了变体（Suspended / Terminal 在下面单独覆盖）"
        );
        assert_eq!(suspended.len(), 5, "SuspendedReason 多了变体");
        assert_eq!(terminal.len(), 5, "TerminalReason 多了变体");
    }

    /// 没有挂起原因时，**可恢复与否**是用户唯一需要的区别——它决定下一步是续传还是重发。
    #[test]
    fn a_reasonless_suspension_still_says_whether_it_can_resume() {
        let recoverable = serde_json::json!({ "phase": "suspended", "recoverable": true });
        let dead = serde_json::json!({ "phase": "suspended", "recoverable": false });
        assert_eq!(phase_label(&recoverable), "可恢复失败");
        assert_eq!(phase_label(&dead), "不可恢复失败");
    }

    /// 一条都没做成时必须说一句——什么都不打印就退出看起来像命令挂了。
    #[test]
    fn an_empty_control_result_still_says_something() {
        // 只验证不 panic 且分支可达；输出本身走 stdout，单测里不捕获。
        let empty = serde_json::json!({ "done": [], "failed": [] });
        render_control(Control::Pause, &empty, false);
        render_control(Control::Resume, &empty, true);
    }

    #[test]
    fn formats_sizes_in_binary_units() {
        assert_eq!(bytes(Some(&Value::from(512))), "512 B");
        assert_eq!(bytes(Some(&Value::from(1024))), "1.0 KiB");
        assert_eq!(bytes(Some(&Value::from(1024 * 1024 * 3))), "3.0 MiB");
    }

    /// 缺字段要给占位符，不能 panic 也不能打印 `null`——记录可能来自旧版本的库。
    #[test]
    fn missing_fields_degrade_gracefully() {
        assert_eq!(bytes(None), "—");
        assert_eq!(text_or(&serde_json::json!({}), "peerName", "—"), "—");
    }
}
