//! 订阅面的线格式。
//!
//! ## 这是对外契约，不是内部类型
//!
//! 消费方（agent harness 的插件）会把这些事件**持久化**进跨月留存、会被回放的记录。
//! 因此三条约束比代码整洁重要得多：
//!
//! 1. **每条带 schema 版本**（[`SCHEMA_VERSION`]）。握手协商覆盖不了「三个月前写下的
//!    那一行」。
//! 2. **每条带订阅内单调 `seq`**，消费方仅凭跳变即可判定漏读，不必信任任何自述计数。
//! 3. **绝不转发领域事件本身**。理由见下。
//!
//! ## 为什么不 `serde_json::to_value(&CoreEvent)`
//!
//! 三条独立理由，任一条都足够：
//!
//! - **会泄露配对凭证**：`PairingRequestReceived` 把 `PairingRequest` flatten 进事件，
//!   而 `PairingMethod::Invite` 携带 128bit bearer capability **明文**——那个类型的注释
//!   自己写着「明文不落盘」，而这条流的终点正是落盘。
//! - **会泄露正文**：文本条目的 `title` 就是正文的前 160 字节，所以也不能直接复用
//!   `InboxItemSummary`。
//! - **`CoreEvent` 的 serde 实现在生产里从未被执行过**——四个宿主全是 match 重映射。
//!   直接透传等于让一个未经检验的 `tag` + `flatten` 组合当场变成对外契约。
//!
//! ## 传输类为什么只有两条
//!
//! 领域侧有六个窄边沿事件（accepted / rejected / paused / resumed / completed / failed），
//! 但它们全是会话投影的窄投影：投影在**创建、每次阶段变化、终态**三种时机都会发，且是唯一
//! 同时带对端、阶段、终态原因与**机器可读失败码**的载荷。而 `TransferFailed.error` 是自由
//! 文本（本仓为此栽过一次：移动端跑英文正则误判），`TransferAccepted` 只有一个 session_id。
//!
//! 暴露六条只会让消费方拿到同一件事的两条记录并被迫去重——那正是「同一条规则的第二份
//! 实现」在契约层的形态。
//!
//! ## 载荷类型的两种写法，判据是「这个值会不会被搬来搬去」
//!
//! 会的用具名类型 + newtype 变体（[`Baseline`] / [`TransferEntry`] / [`ProgressSample`]）
//! ——它们要在基线装配与降频折叠之间传递，一个具名类型省掉每处的解构与重组；
//! 只在 [`translate`] 里构造一次、字段又只有两三个的留结构体变体。
//! 内部标签枚举下两者的 JSON **完全一致**，所以这个选择不影响契约。

use serde::Serialize;
use serde_json::{Value, json};
use swarmdrop_core::host::CoreEvent;
use swarmdrop_core::transfer::inbox::InboxItemSummary;
use swarmdrop_core::transfer::store::TransferProjection;
use swarmdrop_host::device::{Device, PairedDeviceInfo};

/// 线格式版本。
///
/// **递增的判据**（spec: `cli-event-stream` 的「事件携带 schema 版本」）：删除字段、
/// 重命名字段、改变既有字段的含义或取值域、改变事件的分类归属。
///
/// **不递增**：新增可选字段、新增事件类型——消费方被要求忽略不认识的字段与类型。
pub const SCHEMA_VERSION: u32 = 1;

/// 给一条事件盖上版本与序号，得到线上真正的那一行。
///
/// ⚠️ **收 [`Value`] 而不是 [`WatchEvent`]，且刻意不反序列化再序列化。** 客户端在这条流上
/// 是**转发者不是解释者**：常驻节点完全可能比它新（升级 CLI 不会重启常驻节点，
/// `swarmdrop update` 之后尤其如此），那时对面发来的事件类型客户端不认识——原样转发才是
/// 对的，反序列化会当场失败，把一条本可以正常流过的记录变成订阅中断。
///
/// `v` / `seq` 在最外层而不是每个变体里：它们对**每一条**都成立，放进变体等于让每个新
/// 事件类型的作者重新决定一次要不要带。
pub fn stamp(seq: u64, event: Value) -> Value {
    let Value::Object(mut fields) = event else {
        // 非对象只可能来自一个坏掉的服务端。包一层让消费方看得见异常，
        // 而不是收到一行结构与其余都不同的记录。
        return json!({ "v": SCHEMA_VERSION, "seq": seq, "kind": "malformed", "payload": event });
    };
    fields.insert("v".into(), SCHEMA_VERSION.into());
    fields.insert("seq".into(), seq.into());
    Value::Object(fields)
}

/// 订阅面的事件。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WatchEvent {
    /// 一次整值快照：**当下**是什么样。
    Baseline(Baseline),

    /// 收件箱多了一条。
    ///
    /// **不含标题与正文**（见模块文档）。要展示就用 `itemId` 去查详情。
    InboxAdded(InboxAdded),
    /// 收件箱条目的归档状态变了。
    InboxArchived { item_id: String, archived: bool },
    /// 收件箱条目被删除。
    InboxRemoved { item_id: String },

    /// 一条传输会话的状态变了（创建、阶段变化、终态都走这条）。
    TransferChanged(TransferEntry),
    /// 传输进度。**聚合、无逐文件明细、按秒降频**（spec: `cli-event-stream`
    /// 的「进度是聚合的、降频的」）。
    TransferProgress(ProgressSample),

    /// 已配对设备的在线状态变了，或配对关系变了。
    ///
    /// 载荷是**变化后的全量设备表**而不是逐条 diff：设备是十几条的量级，全量让消费方
    /// 不必自己维护一份可能与真相分叉的镜像。
    ///
    /// ⚠️ **「全量」指的是全量_已配对_设备**，与 [`Baseline::devices`] 同一口径
    /// （`DeviceFilter::Paired`）。它**只能**由 [`super::serve`] 向节点现取产出，
    /// 不能由某条内核事件的载荷直接转发——内核的 `CoreEvent::DevicesChanged` 带的是
    /// `DeviceFilter::All`，那是**被观测到的 peer 表**，一台已配对但本次运行还没上过线的
    /// 设备根本不在里面。桌面端消费它是对的（它把已配对清单另存一份，只从这里取在线
    /// 状态），照搬到这条以配对表为契约的流上就成了 bug：基线报 1 台、下一条网络事件
    /// 一到就变 0 台，看起来跟「刚刚被解除配对」一模一样。见 [`affects_devices`]。
    DevicesChanged { devices: Vec<DeviceEntry> },

    /// 常驻节点不在了。订阅**不因此结束**，会继续等它回来。
    ///
    /// 没有对称的「节点回来了」：节点一接上就会推一条新的 [`Baseline`]，
    /// 那条既宣告了节点在跑，又把此刻的真实状态一并交出去——再加一个空事件只是
    /// 同一件事的第二种说法。
    NodeUnavailable,

    /// 因为消费方读得太慢，丢弃了若干条**边沿**事件。
    ///
    /// **必须显式给出而不是静默丢**：消费方分辨不了「流结束了」与「我漏了一段」——
    /// 而它会把这段记录长期留存。
    ///
    /// 进度样本被丢弃**不计入**这里：下一帧会纠正它，报进去只会让一次正常的降压
    /// 长得像一次数据损失。
    Truncated { dropped: usize },
}

/// 订阅建立与每次接上节点时的整值快照。
///
/// 收件箱只带最近 N 条，`inboxHasMore` 说明还有更早的——消费方要更早的条目应当按需
/// 检索，而不是指望订阅把整本账搬过来。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub inbox: Vec<InboxEntry>,
    pub inbox_has_more: bool,
    pub devices: Vec<DeviceEntry>,
    pub transfers: Vec<TransferEntry>,
    /// 这条基线是不是由一个在跑的节点给出的。
    ///
    /// 为假时 `devices[].online` 全是 `null`（本机没做过探测），且不会有传输与设备
    /// 事件，直到节点起来——那时会再推一条 `nodeRunning: true` 的基线。
    pub node_running: bool,
}

/// 一条新到达的收件箱条目。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxAdded {
    pub item_id: String,
    pub content_kind: String,
    pub source_peer_id: String,
    pub source_name: String,
    pub item_count: i32,
    pub total_size: i64,
    pub received_at: i64,
    pub transfer_session_id: Option<String>,
}

/// 收件箱条目在基线里的形态。与 [`InboxAdded`] 同样**不含标题与正文**。
///
/// **没有 `archived` 字段，因为基线只列未归档的条目。** 归档正是「我把它收起来了」的
/// 表达，而基线回答的是「此刻手边有什么可用」——把收起来的东西摆回去，等于替用户
/// 撤销了那个动作。归档状态的变化仍有事件；消费方收到一条「取消归档」而手上没有那条
/// 条目时，按需查它的详情，与「要更早的条目就按需检索」是同一条取数原则。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxEntry {
    pub item_id: String,
    pub content_kind: String,
    pub source_name: String,
    pub item_count: i32,
    pub total_size: i64,
    pub received_at: i64,
}

impl From<&InboxItemSummary> for InboxEntry {
    fn from(item: &InboxItemSummary) -> Self {
        Self {
            item_id: item.id.to_string(),
            content_kind: enum_value(&item.content_kind),
            source_name: item.source_name.clone(),
            item_count: item.item_count,
            total_size: item.total_size,
            received_at: item.received_at,
            // ⚠️ `item.title` 就在手边，**不要顺手带上**：文本条目的标题是正文的前
            // 160 字节（`text_preview` 的产物），而这条流的终点是跨月留存的日志。
        }
    }
}

/// 设备在基线与变化事件里的形态。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEntry {
    pub peer_id: String,
    pub name: String,
    /// `None` = 未知（本机节点没跑，没做过探测），**不是**离线。
    ///
    /// 把未知说成离线是一个凭空的断言：用户看到「离线」会去排查网络，看到「未知」
    /// 才会想到「节点没开」。判据与 [`crate::runtime::devices::DeviceRow`] 同源。
    pub online: Option<bool>,
}

impl From<&Device> for DeviceEntry {
    fn from(device: &Device) -> Self {
        Self {
            peer_id: device.peer_id.to_string(),
            name: device.os_info.display_name(),
            online: Some(crate::runtime::devices::is_online(device)),
        }
    }
}

impl From<&PairedDeviceInfo> for DeviceEntry {
    fn from(info: &PairedDeviceInfo) -> Self {
        Self {
            peer_id: info.peer_id.to_string(),
            name: info.os_info.display_name(),
            online: None,
        }
    }
}

/// 把内核的设备表收敛成订阅面认的那份。
///
/// **只留已配对的**：内核推的是 `DeviceFilter::All`——「本次运行发现的 peer」，
/// 局域网里路过的陌生设备也在其中。订阅面承诺的是「已配对设备上下线」
/// （spec: `cli-event-stream`），把陌生设备混进去等于换了一个集合。
pub fn paired_entries(devices: &[Device]) -> Vec<DeviceEntry> {
    sorted(
        devices
            .iter()
            .filter(|device| device.is_paired)
            .map(DeviceEntry::from)
            .collect(),
    )
}

/// 无节点时直读本机记录得到的那份设备表。在线状态一律未知。
pub fn record_entries(infos: &[PairedDeviceInfo]) -> Vec<DeviceEntry> {
    sorted(infos.iter().map(DeviceEntry::from).collect())
}

/// 按标识排序。
///
/// **不是审美**：下游靠「与上一份逐字相同」判定要不要推送（见 [`super::fold::Coalescer`]），
/// 而两条来源的次序都没有契约——次序一抖，每次 ping 都会推一条内容完全一样的变化事件。
fn sorted(mut entries: Vec<DeviceEntry>) -> Vec<DeviceEntry> {
    entries.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    entries
}

/// 传输会话的窄投影。
///
/// 刻意不带 `files`：几万文件的目录传输会让每条事件都携带一个巨大的数组，而消费方
/// 要明细时按需查即可。
///
/// ⚠️ **三个「为什么结束」的字段一个都不能省。** `phase == "terminal"` 只说明它结束了，
/// 而完成、被取消、对方拒绝、没来得及处理在消费方那里是四件不同的事；`failure` 也分辨
/// 不了它们（正常完成与被取消都没有失败码）。少了它们，「传输结束了」这条事件就只剩
/// 一半信息，消费方只能回头再查一次。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferEntry {
    pub session_id: String,
    pub direction: String,
    pub peer_name: String,
    pub phase: String,
    /// 挂起的原因（`phase == "suspended"` 时有值）。
    pub suspended_reason: Option<String>,
    /// 终态的原因（`phase == "terminal"` 时有值）。
    pub terminal_reason: Option<String>,
    /// 中断之后还续得上吗。
    pub recoverable: bool,
    pub transferred_bytes: i64,
    pub total_bytes: i64,
    pub file_count: usize,
    /// 机器可读的失败判别码（不是给人看的自由文本）。
    ///
    /// **这是本契约里唯一一处直接转发领域类型的地方**，判据是它本身就是一个为机器
    /// 判别而生的窄分类：无正文、无凭证，且带着文案需要的参数（保留天数、拒绝理由）。
    /// 压成一个字符串会把那些参数丢掉，而消费方复原不出来。
    pub failure: Option<Value>,
    pub updated_at: i64,
}

impl From<&TransferProjection> for TransferEntry {
    fn from(p: &TransferProjection) -> Self {
        Self {
            session_id: p.session_id.to_string(),
            direction: enum_value(&p.direction),
            peer_name: p.peer_name.clone(),
            phase: enum_value(&p.phase),
            suspended_reason: p.suspended_reason.as_ref().map(enum_value),
            terminal_reason: p.terminal_reason.as_ref().map(enum_value),
            recoverable: p.recoverable,
            transferred_bytes: p.transferred_bytes,
            total_bytes: p.total_size,
            file_count: p.files.len(),
            failure: p
                .failure
                .as_ref()
                .and_then(|f| serde_json::to_value(f).ok()),
            updated_at: p.updated_at,
        }
    }
}

/// 一帧聚合进度。
///
/// 有具名类型是因为它要在折叠表里按会话存放（[`super::fold::Coalescer`]），
/// 而不只是构造一次就发走。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSample {
    pub session_id: String,
    pub direction: String,
    pub transferred_bytes: i64,
    pub total_bytes: i64,
    pub completed_files: u32,
    pub total_files: u32,
}

/// 把一个 serde 枚举取成它的线上字符串。
///
/// **不手写 match**：这些枚举的字符串表示已经由 serde 的 `rename_all` 定死，手写一份
/// 会在新增变体时静默漂移（不会编译失败，只会在消费方那里变成一个谁也不认识的值）。
fn enum_value<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 把领域事件翻译成订阅面事件。
///
/// 返回 `None` = 这条与订阅面无关（网络状态、准备进度、配对请求等），**静默跳过是对的**：
/// 订阅面只承诺覆盖收件箱 / 传输 / 设备三类。
pub fn translate(event: &CoreEvent) -> Option<WatchEvent> {
    match event {
        CoreEvent::InboxItemAdded { event } => Some(WatchEvent::InboxAdded(InboxAdded {
            item_id: event.item_id.to_string(),
            content_kind: enum_value(&event.content_kind),
            source_peer_id: event.source_peer_id.clone(),
            source_name: event.source_name.clone(),
            item_count: event.item_count,
            total_size: event.total_size,
            received_at: event.received_at,
            transfer_session_id: event.transfer_session_id.map(|id| id.to_string()),
        })),
        CoreEvent::InboxItemArchived { event } => Some(WatchEvent::InboxArchived {
            item_id: event.item_id.to_string(),
            archived: event.archived,
        }),
        CoreEvent::InboxItemRemoved { event } => Some(WatchEvent::InboxRemoved {
            item_id: event.item_id.to_string(),
        }),

        CoreEvent::TransferProjection { projection } => {
            Some(WatchEvent::TransferChanged(TransferEntry::from(projection)))
        }
        CoreEvent::TransferProgress { event } => {
            Some(WatchEvent::TransferProgress(ProgressSample {
                session_id: event.session_id.to_string(),
                direction: enum_value(&event.direction),
                transferred_bytes: event.transferred_bytes as i64,
                total_bytes: event.total_bytes as i64,
                completed_files: event.completed_files as u32,
                total_files: event.total_files as u32,
            }))
        }

        // ⚠️ `CoreEvent::DevicesChanged` **刻意不在这里翻译**——它只是「设备表可能变了」
        // 的信号，载荷的口径不对（判据见 [`WatchEvent::DevicesChanged`] 与
        // [`affects_devices`]）。它由 [`super::serve::forward`] 现取全表产出。
        //
        // 其余与订阅面无关：网络状态、准备进度、配对请求、文本注意力（收件箱事件已覆盖
        // 「收件箱变了」这件事，注意力信号是另一个问题）等。
        _ => None,
    }
}

/// 丢掉这条要不要如实上报（spec: `cli-event-stream` 的「边沿事件不得静默丢失」）。
///
/// 两个条件都要满足：
///
/// 1. **它不是采样。** 采样类只有进度——下一帧会纠正它，丢了不留痕迹。判据写成「只有
///    进度是采样」而不是逐条列举边沿，是为了让新增的领域事件默认落进「丢了要上报」
///    一侧；反过来写会让下一个人加的事件静默消失。
/// 2. **它真会出现在这条流上。** 领域事件里有一大半与订阅面无关（`PrepareProgress`、
///    `NetworkStatusChanged`、配对请求……），它们进队列只是路过。丢掉一条路过的却报一次
///    截断，是在告诉消费方「你的记录有个洞」——而那个洞从来不存在，且它会把这段跨月
///    留存的记录标记成不完整。发一个大目录时 `PrepareProgress` 就能刷满队列，那正是
///    最容易撞上的场景。
///
/// 判据问 [`produces_frame`]，**不另列一张变体表**：两张表迟早分叉，而分叉的表现正是
/// 上面那条凭空的截断。代价是队列满时多跑一次翻译，那只发生在丢弃的那一刻。
pub fn report_loss(event: &CoreEvent) -> bool {
    !matches!(event, CoreEvent::TransferProgress { .. }) && produces_frame(event)
}

/// 这条事件会不会在订阅上产出一帧。
///
/// 产帧的路径有两条——[`translate`] 直接翻译，[`affects_devices`] 触发现取全表——
/// 所以「会不会产帧」必须两条都问。少问一条，[`report_loss`] 就会把一次真实的丢失
/// 判成路过，消费方的设备表从此静悄悄地停在旧值上。
pub fn produces_frame(event: &CoreEvent) -> bool {
    translate(event).is_some() || affects_devices(event)
}

/// 这条事件是否意味着「已配对设备表可能变了」——**新表一律现取，不用事件载荷**。
///
/// 三类事件在这里同等对待，理由各不相同：
///
/// - `PairedDeviceAdded` / `PairedDeviceRemoved` / `DeviceRenamed` **没带新表**，
///   只带一个 `peer_id`。
/// - `CoreEvent::DevicesChanged` **带了表，但口径不对**：它是 `DeviceFilter::All`，
///   即被观测到的 peer；一台已配对而本次运行没上过线的设备不在其中。直接转发它，
///   订阅上就会出现「基线 1 台 → 一条网络事件之后 0 台」。
///
/// **不能指望内核的 `DevicesChanged` 兜住前三条**：那条由网络事件驱动（ping 成功、
/// 连接变化），而解除一台**离线**设备的配对不产生任何网络事件——不补这一下，那次解除
/// 会在订阅上无限期不可见。
pub fn affects_devices(event: &CoreEvent) -> bool {
    matches!(
        event,
        CoreEvent::PairedDeviceAdded { .. }
            | CoreEvent::PairedDeviceRemoved { .. }
            | CoreEvent::DeviceRenamed { .. }
            | CoreEvent::DevicesChanged { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(session: &str) -> ProgressSample {
        ProgressSample {
            session_id: session.into(),
            direction: "receive".into(),
            transferred_bytes: 1,
            total_bytes: 2,
            completed_files: 3,
            total_files: 4,
        }
    }

    /// **进度事件的载荷大小必须与文件数无关**（spec: `cli-event-stream` 的「大目录传输」）。
    ///
    /// 它是这条订阅上唯一的高频事件，而消费方会逐条持久化。领域侧已经为同一笔账算过一次：
    /// 每帧克隆整个文件向量并跨进程序列化，收一个几万文件的目录时光是自家事件流就能把
    /// 接收吞吐吃光。
    #[test]
    fn progress_payload_carries_no_per_file_detail() {
        let json =
            serde_json::to_string(&WatchEvent::TransferProgress(sample("s"))).expect("序列化");
        // 断言的是**逐文件数组**不在，不是「字段名里没有 files」——`completedFiles` /
        // `totalFiles` 是聚合计数，它们正是这条事件该有的东西。
        assert!(
            !json.contains('['),
            "进度事件不得携带任何数组（逐文件明细会让载荷随文件数膨胀）: {json}"
        );
    }

    /// newtype 变体的 JSON 与结构体变体**逐字相同**——内部标签枚举把内层字段并进外层。
    ///
    /// 这条钉的是契约不受实现写法影响：把某个变体在两种写法之间改来改去不该动到线格式。
    #[test]
    fn a_newtype_variant_flattens_into_the_tagged_object() {
        let json =
            serde_json::to_value(WatchEvent::TransferProgress(sample("abc"))).expect("序列化");
        assert_eq!(json["kind"], "transferProgress");
        assert_eq!(json["sessionId"], "abc");
        assert!(
            json.get("0").is_none(),
            "不得出现 newtype 的位置字段: {json}"
        );
    }

    /// 每一行都带版本与序号——消费方仅凭事件自身就能判定格式与漏读。
    #[test]
    fn every_line_carries_version_and_seq() {
        let line = stamp(
            7,
            serde_json::to_value(WatchEvent::NodeUnavailable).expect("序列化"),
        );
        assert_eq!(line["v"], SCHEMA_VERSION);
        assert_eq!(line["seq"], 7);
        assert_eq!(line["kind"], "nodeUnavailable");
    }

    /// **不认识的事件类型必须原样流过。**
    ///
    /// 常驻节点可能比客户端新（升级 CLI 不重启常驻节点）。客户端若先反序列化成
    /// [`WatchEvent`] 再盖章，那一刻会失败，一次本可正常流过的记录变成订阅中断。
    #[test]
    fn an_unknown_event_kind_still_gets_stamped() {
        let future = json!({ "kind": "somethingNewer", "detail": 1 });
        let line = stamp(3, future);
        assert_eq!(line["kind"], "somethingNewer");
        assert_eq!(line["detail"], 1);
        assert_eq!(line["seq"], 3);
    }

    /// 与订阅面无关的领域事件静默跳过，不是错误。
    #[test]
    fn unrelated_events_are_skipped() {
        assert!(
            translate(&CoreEvent::Error {
                message: "x".into()
            })
            .is_none()
        );
    }

    fn progress_event() -> CoreEvent {
        use swarmdrop_core::transfer::progress::{RuntimeTransferDirection, TransferProgressEvent};

        CoreEvent::TransferProgress {
            event: TransferProgressEvent {
                session_id: uuid::Uuid::new_v4(),
                direction: RuntimeTransferDirection::Send,
                total_files: 1,
                completed_files: 0,
                total_bytes: 1,
                transferred_bytes: 0,
                speed: 0.0,
                eta: None,
                files: Vec::new(),
            },
        }
    }

    /// **进度丢了不上报**——下一帧会纠正它，报进去会让一次正常的降压长得像数据损失。
    #[test]
    fn a_dropped_progress_frame_is_not_reported() {
        assert!(!report_loss(&progress_event()));
    }

    /// 内核的 `DevicesChanged` **不得被直接翻译成订阅面的同名事件**。
    ///
    /// 它带的是 `DeviceFilter::All`——被观测到的 peer 表。一台已配对但本次运行还没
    /// 上过线的设备不在其中，于是转发它就等于把那台设备从订阅上抹掉：基线刚报过 1 台，
    /// 下一条 ping 成功事件一到就变 0 台。实测踩过（`dsh-swarmdrop` 的设备面板整片
    /// 消失），修法是让 [`super::serve::forward`] 现取全表，判据落在
    /// [`affects_devices`] 上。
    #[test]
    fn the_kernel_device_event_carries_no_table_of_its_own() {
        let event = CoreEvent::DevicesChanged {
            devices: Vec::new(),
        };
        assert!(translate(&event).is_none(), "载荷口径不对，不能直接翻译");
        assert!(affects_devices(&event), "但它仍要触发一次现取");
    }

    /// 现取那条路径产出的帧也是帧，丢了要如实上报。
    ///
    /// [`report_loss`] 早先只问 [`translate`]；`DevicesChanged` 从那条路搬走之后，
    /// 只问一条就会把一次真实的设备表丢失判成「路过」，消费方的设备表从此停在旧值上
    /// 而没有任何人被告知。
    #[test]
    fn a_dropped_device_event_is_reported() {
        assert!(report_loss(&CoreEvent::DevicesChanged {
            devices: Vec::new(),
        }));
    }

    /// **与订阅面无关的事件丢了也不上报。**
    ///
    /// 它们进队列只是路过，压根不会出现在这条流上。丢一条路过的却报一次截断，
    /// 等于告诉消费方「你的记录有个洞」——而那个洞从来不存在，且它会把这段跨月留存的
    /// 记录标记成不完整。发大目录时 `PrepareProgress` 就能刷满队列，这不是罕见场景。
    #[test]
    fn a_dropped_irrelevant_event_is_not_reported() {
        assert!(!report_loss(&CoreEvent::Error {
            message: "x".into()
        }));
    }

    /// 真正的边沿事件丢了**必须**上报。
    #[test]
    fn a_dropped_edge_event_is_reported() {
        assert!(report_loss(&CoreEvent::InboxItemRemoved {
            event: swarmdrop_core::transfer::inbox::InboxItemRemovedEvent {
                item_id: uuid::Uuid::new_v4(),
            },
        }));
    }
}
