//! `transfer watch` 的实时面板。
//!
//! **本层不含业务判断**（见 [`super`]）：哪些会话该出现在面板上、按了热键之后做什么，
//! 都由 [`crate::cmd::transfer`] 决定；这里只把已经算好的一组记录画成一屏进度条，
//! 并在每次刷新时与上一屏对齐。
//!
//! 画在 **stderr**：面板是过程信息不是命令结果，而 `--json` 模式下 stdout 只能有
//! 可解析的输出（那个模式下压根不构造 [`Panel`]，见 `cmd::transfer`）。

use std::collections::{HashMap, HashSet};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde_json::Value;

use crate::runtime::transfers::PHASE_ACTIVE;

use super::transfer::{direction_glyph, phase_label};
use super::{short, text_or};

/// 正在传的那条。
///
/// 速率与剩余时间是用户真正在等的答案——传一个大文件时，「还要多久」比「传了多少」
/// 更常被问起。
const ACTIVE_TEMPLATE: &str =
    "{prefix} {bar:20.cyan/blue} {percent:>3}%  {done}/{total}  {rate}  剩余 {eta}";

/// 没在传的那条（等待接受、已暂停、已中断）。
///
/// **刻意不画速率与剩余时间**：此刻它们是「0 B/s」与「剩余 ∞」，印出来是在报告一个
/// 假事实——用户会以为传输卡住了，而它其实只是在等对方点确认。位置让给状态本身。
const IDLE_TEMPLATE: &str = "{prefix} {bar:20.dim} {percent:>3}%  {done}/{total}  {msg}";

/// 顶行：只有一句话，没有进度条。
const HEADER_TEMPLATE: &str = "{msg}";

/// 对端名字在前缀里最多占几个字符。
///
/// 截断而不是任其自然长度：一屏里每条进度条的起点应当大致对齐，而设备名是唯一
/// 长度不可控的部分（`DeviceName::MAX_CHARS` 是 40）。
const NAME_CHARS: usize = 14;

/// 面板上的一条会话。
struct Row {
    bar: ProgressBar,
    /// 当前挂的是不是 [`ACTIVE_TEMPLATE`]。
    ///
    /// 记住它是为了**只在阶段真的变了时才换样式**：`ProgressStyle::with_template`
    /// 每次调用都要重新解析模板，而面板每秒刷新一次、每条会话都要过一遍。
    active: bool,
}

/// 一屏进度条，按会话标识与上一屏增量对齐。
///
/// 生命周期就是「面板显示期间」：要腾出屏幕（弹选择菜单）时**整个丢掉再重建**，
/// 而不是加一个 `hide()`/`show()`。清屏与重画的正确顺序只写在 [`Drop`] 里一处，
/// 两个方法则意味着两条各自可能写错的路径。
pub struct Panel {
    multi: MultiProgress,
    header: ProgressBar,
    rows: HashMap<String, Row>,
    /// 顶行要不要带热键说明。
    hotkeys: bool,
}

impl Panel {
    /// `hotkeys` 为假（此刻问不了人）时顶行只报条数——**不能印一行按了没反应的提示**。
    pub fn new(hotkeys: bool) -> Self {
        let multi = MultiProgress::new();
        let header = multi.add(ProgressBar::no_length());
        header.set_style(template(HEADER_TEMPLATE));
        Self {
            multi,
            header,
            rows: HashMap::new(),
            hotkeys,
        }
    }

    /// 把面板对齐到这一组记录。
    ///
    /// 增量而不是「清空重画」：进度条的速率与剩余时间由 indicatif 在**同一个** bar 上
    /// 维护一个时间窗口算出来，每轮换一个新 bar 会让它们永远停在初始值。
    pub fn sync(&mut self, records: &[Value]) {
        self.header.set_message(self.header_line(records.len()));

        let mut alive = HashSet::with_capacity(records.len());
        for record in records {
            let id = text_or(record, "sessionId", "");
            // 形状不对的行不画也不 panic：面板可能在读一个更新过的常驻节点。
            if id.is_empty() {
                continue;
            }
            alive.insert(id.clone());

            let active = is_active(record);
            // 借出 `multi` 再进 entry：闭包里直接写 `self.multi` 会与 `self.rows` 的
            // 可变借用撞车（同一个 `self`，编译器分不开）。
            let multi = &self.multi;
            let row = self.rows.entry(id).or_insert_with(|| Row {
                // 追加而非插入：顶行是第一个加进去的，新会话排在它后面，
                // 已有会话的位置不动。
                bar: multi.add(ProgressBar::no_length()),
                // 新 bar 的样式在下面统一设，这里给一个与之相反的值确保那一步会执行。
                active: !active,
            });
            if row.active != active {
                row.bar.set_style(template(if active {
                    ACTIVE_TEMPLATE
                } else {
                    IDLE_TEMPLATE
                }));
                row.active = active;
            }
            row.bar.set_prefix(prefix_of(record));
            row.bar.set_message(phase_label(record));
            row.bar.set_length(bytes(record, "totalSize"));
            row.bar.set_position(bytes(record, "transferredBytes"));
        }

        self.retire(&alive);
    }

    /// 在面板**上方**留一行话（动作结果、失败原因）。
    ///
    /// 走 `println` 而不是塞进顶行：这些是一次性的事件，应当像日志一样往上滚走，
    /// 而顶行表达的是「此刻的状态」。
    pub fn note(&self, line: &str) {
        let _ = self.multi.println(line);
    }

    /// 收掉这一轮不再出现的那几条（传完了、被取消了、记录被删了）。
    fn retire(&mut self, alive: &HashSet<String>) {
        let stale: Vec<String> = self
            .rows
            .keys()
            .filter(|id| !alive.contains(*id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(row) = self.rows.remove(&id) {
                row.bar.finish_and_clear();
                self.multi.remove(&row.bar);
            }
        }
    }

    /// 顶行。
    fn header_line(&self, count: usize) -> String {
        let what = if count == 0 {
            "没有正在进行的传输".to_owned()
        } else {
            format!("{count} 条传输进行中")
        };
        if self.hotkeys {
            format!("{what}   [p] 暂停  [r] 恢复  [c] 取消  [q] 退出")
        } else {
            what
        }
    }
}

/// 离开作用域即把面板从屏幕上收干净。
///
/// 收在 `Drop` 而不是一个 `finish()` 方法：面板有好几条离开路径（用户按 q、Ctrl-C、
/// 取数失败、弹菜单前腾地方），漏掉其中一条的表现是**半屏残留的进度条压在后续输出
/// 上面**——而那正是 `swarmdrop send` 的进度条曾经犯过的错（见 `render::send`）。
impl Drop for Panel {
    fn drop(&mut self) {
        for row in self.rows.values() {
            row.bar.finish_and_clear();
        }
        self.header.finish_and_clear();
        let _ = self.multi.clear();
    }
}

/// 编译一个模板；写错时退回默认样式。
///
/// **退化是静默的**——进度条照常出现，只是速率与剩余时间不见了。三个模板各有一条
/// 断言测试盯着它们能解析，见本模块的 `tests`。
fn template(source: &str) -> ProgressStyle {
    super::send::with_byte_keys(
        ProgressStyle::with_template(source).unwrap_or_else(|_| ProgressStyle::default_bar()),
    )
}

/// 这条会话此刻在不在传字节。
///
/// **只认 `active`**：等待接受与已暂停都还没有（或已经没有）数据在流动，给它们画
/// 速率等于报告一个假事实。判据与 `runtime::transfers::Control` 的 `Pause` 一致，
/// 但那边决定的是「能不能暂停」，这边决定的是「画哪套样式」——同一个 phase，两个问题。
fn is_active(record: &Value) -> bool {
    record.get("phase").and_then(Value::as_str) == Some(PHASE_ACTIVE)
}

/// 一行的前缀：方向 + 对端名。
///
/// 名字按**字符**截断（不是字节），中文名不会被切坏。⚠️ 这不能保证像素级对齐——
/// 中文在终端里占两列，而 `{:<}` 数的是字符。它要的只是「大致成列」，
/// 真正要读的百分比与速率在后面。
fn prefix_of(record: &Value) -> String {
    let name = text_or(record, "peerName", "");
    format!(
        "{} {:<NAME_CHARS$}",
        direction_glyph(record.get("direction").and_then(Value::as_str)),
        short(super::blank_as_placeholder(&name), NAME_CHARS),
    )
}

/// 取一个字节数字段，缺失或为负时当 0。
///
/// **不能给「—」**：这里的消费者是 `set_length` / `set_position`，它们只吃数。
fn bytes(record: &Value, key: &str) -> u64 {
    record.get(key).and_then(Value::as_i64).unwrap_or(0).max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 三个模板都必须可解析。
    ///
    /// 写错只会在运行时**静默**退回默认样式：进度条照常出现，速率与剩余时间不见了，
    /// 没有任何报错。
    #[test]
    fn every_template_parses() {
        for source in [ACTIVE_TEMPLATE, IDLE_TEMPLATE, HEADER_TEMPLATE] {
            assert!(
                ProgressStyle::with_template(source).is_ok(),
                "模板无法解析，样式会静默退回默认: {source}"
            );
        }
    }

    /// 只有 `active` 才画速率——其余阶段没有字节在流动。
    #[test]
    fn only_active_sessions_get_the_rate_template() {
        assert!(is_active(&json!({ "phase": "active" })));
        for phase in ["offered", "waiting_accept", "suspended", "terminal"] {
            assert!(!is_active(&json!({ "phase": phase })), "{phase} 不该画速率");
        }
        assert!(!is_active(&json!({})));
    }

    /// 缺字段的行要能画出来而不是 panic——面板可能在读一个更新过的常驻节点。
    #[test]
    fn a_bare_record_still_renders() {
        let bare = json!({});
        assert_eq!(bytes(&bare, "totalSize"), 0);
        // 负数（不该出现，但记录来自另一个进程）不得回绕成一个天文数字。
        assert_eq!(bytes(&json!({ "totalSize": -1 }), "totalSize"), 0);
        assert!(prefix_of(&bare).contains("（未命名设备）"));
    }

    /// 顶行在没有热键时**不得**印一行按了没反应的提示。
    ///
    /// 这条只在两种环境的差异上显形：管道里画不出面板、也读不到键，而顶行如果照旧
    /// 写着「[q] 退出」，看日志的人会以为这条命令卡在等按键。
    #[test]
    fn the_header_only_offers_keys_it_can_honour() {
        let with = Panel::new(true);
        let without = Panel::new(false);

        assert!(with.header_line(2).contains("[q] 退出"));
        assert!(
            !without.header_line(2).contains('['),
            "{}",
            without.header_line(2)
        );
        assert!(without.header_line(0).contains("没有正在进行的传输"));
    }

    /// 面板要能对齐到任意一组记录、再收干净，全程不 panic。
    ///
    /// 覆盖三条增量路径：新增、更新（含阶段切换时换样式）、退场。
    #[test]
    fn the_panel_tracks_sessions_in_and_out() {
        let mut panel = Panel::new(false);
        let a = json!({ "sessionId": "a", "phase": "active", "totalSize": 100, "transferredBytes": 10 });
        let b =
            json!({ "sessionId": "b", "phase": "suspended", "suspendedReason": "local_paused" });

        panel.sync(&[a.clone(), b.clone()]);
        assert_eq!(panel.rows.len(), 2);

        // 同一条会话换了阶段：样式要跟着换，bar 不能重建（重建会把速率窗口清零）。
        let paused_a = json!({ "sessionId": "a", "phase": "suspended", "totalSize": 100, "transferredBytes": 60 });
        panel.sync(&[paused_a]);
        assert_eq!(panel.rows.len(), 1, "b 结束了就该从面板上消失");
        assert!(!panel.rows["a"].active);

        // 没有 sessionId 的行不占位、也不 panic。
        panel.sync(&[json!({ "phase": "active" })]);
        assert!(panel.rows.is_empty());
    }
}
