//! `WebNode`：Web 壳的 wasm-bindgen API 面。
//!
//! 浏览器节点**包一层 core 的组合根** [`start_node`]（与桌面/移动同源装配），注入 Browser
//! [`EndpointProfile`] + Web 端口实现（IndexedDB 写穿 store / OPFS / WebEventSink transfer
//! 事件流）。走完整 [`NetManager`] + 3 协议 Router（含 pairing）：invite 配对经
//! `pair_with_invite`，配对设备记录与传输会话都经 IndexedDB 持久化、在 `spawn()` 时恢复
//! （会话恢复的范围与理由见 [`crate::store`]）。NetManager 侧 pairing/device 事件走最小
//! [`WebEventBus`](crate::event_bus)（consume-invite 路径的确认在邀请方桌面，浏览器侧暂不 surface）。

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use swarmdrop_core::device_manager::DeviceFilter;
use swarmdrop_core::host::EventBus;
use swarmdrop_core::network::event_loop::spawn_event_loop;
use swarmdrop_core::network::{CandidateRoles, NetManager, NetworkRuntimeConfig};
use swarmdrop_core::protocol::pairing::{PairingRefuseReason, PairingResponse};
use swarmdrop_core::runtime::{EndpointProfile, HostPorts, NodeCredentials, start_node};
use swarmdrop_host::device::{DeviceName, DeviceReceivePolicy, DeviceTrustLevel};
use swarmdrop_host::{CoreSaveLocation, FileAccess, FileSourceId, PairedDeviceStore};
use swarmdrop_invite::{
    InviteParseError, PairInvite, TransportPolicy, capability_hash_from_hex, capability_hash_to_hex,
};
use swarmdrop_net::{Endpoint, NodeAddr, NodeId, RelayState, SecretKey};
use swarmdrop_transfer::coordinator::TransferCoordinator;
use swarmdrop_transfer::events::TransferEventSink;
use swarmdrop_transfer::manager::TransferManager;
use swarmdrop_transfer::protocol::TransferOrigin;
use swarmdrop_transfer::store::TransferStore;
use swarmdrop_transfer::{HostEnumeratedFile, SUSPENDED_RECEIVE_RETENTION_SECS};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::File;

use crate::error::{WebError, js_err};
use crate::event_bus::{PendingPairings, WebEventBus};
use crate::events::WebEventSink;
use crate::file_access::OpfsFileAccess;
use crate::identity;
use crate::paired_devices::WebPairedDeviceStore;
use crate::store::{WebStore, WebTransferStore};
use crate::types::{
    ConnectionJson, InviteListItemJson, OfferJson, PairInvitePreviewJson, PairingOutcomeJson,
    PairingRefusedJson, PendingPairingJson,
};

/// 浏览器连接与 reservation 的最大等待时间。超时会通过 Endpoint 的取消路径
/// 停止无主拨号/relay 重试，不是只 reject JavaScript Promise。
const REACHABILITY_TIMEOUT: Duration = Duration::from_secs(20);

// specta 导出的 TS 类型（bindings/bindings.ts，由 `cargo test -p swarmdrop-web
// --features specta` 生成并入库）整体注入 .d.ts，供下方 typescript_type 引用——
// wasm-bindgen 默认把 JsValue 返回值标成 any，这里把方法签名接到具名类型上。
#[wasm_bindgen(typescript_custom_section)]
const TS_BINDINGS: &'static str = include_str!("../bindings/bindings.ts");

#[wasm_bindgen]
extern "C" {
    /// `pending_offers()` 的返回：`OfferJson[]`。
    #[wasm_bindgen(typescript_type = "OfferJson[]")]
    pub type OfferJsonArray;
    /// `pending_pairing_requests()` 的返回：`PendingPairingJson[]`。
    #[wasm_bindgen(typescript_type = "PendingPairingJson[]")]
    pub type PendingPairingArray;
    /// `connect_invite()` 的返回。
    #[wasm_bindgen(typescript_type = "PairingOutcomeJson")]
    pub type PairingOutcomeJs;
    /// `events()` 的返回：逐条产出 [`WebTransferEvent`] 序列化对象的流。
    #[wasm_bindgen(typescript_type = "ReadableStream<WebTransferEvent>")]
    pub type TransferEventStream;
    /// `connect()` 的返回。
    #[wasm_bindgen(typescript_type = "ConnectionJson")]
    pub type ConnectionJsonJs;
    /// `paired_devices()` 的返回：`Device[]`。
    #[wasm_bindgen(typescript_type = "Device[]")]
    pub type DeviceArray;
    /// `infra_links()` 的返回：`InfraLink[]`。
    #[wasm_bindgen(typescript_type = "InfraLink[]")]
    pub type InfraLinkArray;
    /// `list_invites()` 的返回：`InviteListItemJson[]`。
    #[wasm_bindgen(typescript_type = "InviteListItemJson[]")]
    pub type InviteListArray;
    /// `decode_invite_preview()` 的返回。
    #[wasm_bindgen(typescript_type = "PairInvitePreviewJson")]
    pub type PairInvitePreviewJs;
    /// `infra_changed()` 的返回：每次 relay 状态变化产出一份全量 `InfraLink` 快照。
    #[wasm_bindgen(typescript_type = "ReadableStream<InfraLink[]>")]
    pub type InfraChangedStream;
    /// `transfer_history()` 的返回：`TransferProjection[]`。
    #[wasm_bindgen(typescript_type = "TransferProjection[]")]
    pub type TransferProjectionArray;
    /// `inbox_items()` 的返回：`InboxItemDetail[]`（列表直接带文件清单与传输投影，
    /// 前端不必再逐条 `inbox_item()`）。
    #[wasm_bindgen(typescript_type = "InboxItemDetail[]")]
    pub type InboxItemDetailArray;
    /// `search_inbox()` 的返回：`InboxSearchHit[]`。
    #[wasm_bindgen(typescript_type = "InboxSearchHit[]")]
    pub type InboxSearchHitArray;
    /// `inbox_item()` / `inbox_item_by_session()` 的返回（无匹配时为 `null`）。
    #[wasm_bindgen(typescript_type = "InboxItemDetail | null")]
    pub type InboxItemDetailJs;

    // ── 入参侧 ──
    //
    // 上面全是返回值；下面两个是**入参**。同一套机制反过来用：`typescript_type` 把
    // wasm-bindgen 默认的 `any` 换成具名类型，于是前端传错形状是编译错误而不是运行时
    // 一句 serde 报错。两个类型都已经在 bindings.ts 里（`Device` 的传递类型），
    // 不需要新增 specta 注册。
    /// `update_paired_device_policy()` 的信任级别入参。
    #[wasm_bindgen(typescript_type = "DeviceTrustLevel")]
    pub type DeviceTrustLevelJs;
    /// `update_paired_device_policy()` 的收件策略入参。
    #[wasm_bindgen(typescript_type = "DeviceReceivePolicy")]
    pub type DeviceReceivePolicyJs;
}

/// `until_active` 的契约级默认超时：即便调用方不传 signal，Promise 也在
/// 有限时间内 settle（「任何 JS Promise 有限时间 settle」不变量的本地兜底）。
/// 30s 是对外承诺的 API 默认值，不与下层内部常量（supervisor tick 间隔 /
/// connect 超时）联动——调用方要更短的耐心用 `AbortSignal.timeout()` 表达。
const UNTIL_ACTIVE_CAP: Duration = Duration::from_secs(30);

/// 提交前校验的判别码 → JS reject 值。
///
/// **不并进 [`WebError`]**：那七个 kind 说的是「哪一层出了错」，而这六个说的是
/// 「你这条地址哪里不对、该怎么改」。压成 `invalidInput` + 一句自由文本，前端就只能
/// 原样贴出来——而 `UnsupportedTransport` 还带着「本端支持哪些传输」这份用户真正需要的
/// 清单。两者的 `kind` 取值互不重叠，JS 侧同一个 `switch` 分得开。
fn infra_addr_err(e: swarmdrop_core::infra::InfraAddrError) -> JsValue {
    crate::serialize::to_js(&e)
        .unwrap_or_else(|_| WebError::invalid_input(format!("地址校验失败: {e:?}")).into())
}

/// serde 可序列化值 → 具名 TS 类型的 JsValue（`unchecked_into` 到 typescript_type 包装）。
fn to_js_typed<T: serde::Serialize, R: JsCast>(value: &T, what: &str) -> Result<R, JsValue> {
    crate::serialize::to_js(value)
        .map(JsValue::unchecked_into)
        .map_err(|e| WebError::network(format!("序列化{what}失败: {e}")).into())
}

/// 具名 TS 类型的 JsValue → serde 可反序列化值（[`to_js_typed`] 的反向）。
///
/// **错误归 `invalidInput` 而不是 `network`**：走到这里说明调用方递进来的形状与 `.d.ts`
/// 声明的对不上，那是调用方的问题。出站方向的 `to_js_typed` 归 `network` 是因为那种失败
/// 只可能是本机序列化器出了岔子，与调用方无关——两个方向的归因本来就不同。
fn from_js_typed<T: for<'de> serde::Deserialize<'de>>(
    value: JsValue,
    what: &str,
) -> Result<T, JsValue> {
    crate::serialize::from_js(value)
        .map_err(|e| WebError::invalid_input(format!("解析{what}失败: {e}")).into())
}

/// 检索条数上限（[`INBOX_SEARCH_LIMIT`](swarmdrop_transfer::inbox::INBOX_SEARCH_LIMIT) 的只读镜像）。
///
/// `search_inbox` 的 `limit` 缺省就取这个值、传大了也会被钳回来，前端**不需要**传它。
/// 导出它只为一件事：UI 要说「只显示了最近 N 条」时得知道 N 是几。
///
/// 换句话说前端仍然不许自带这个数字——那正是 #111 修掉的分叉（此前四个宿主四个值，
/// 而截断掉的永远是最早收到的那批）。wasm-bindgen 不导出常量，所以包成函数。
/// 某信任级别的默认接收策略。
///
/// **纯派生，不碰节点**，所以是自由函数不是 `WebNode` 方法——它在节点还没起来时也该能用
/// （信任策略对话框可以先开着）。
///
/// 存在的全部理由是**不让 JS 再抄一份那张表**。桌面与移动此前各抄了一份，两份还长出了不同的
/// 「切级别时保留哪些字段」规则，而内核那一份一个都不保留——同一个产品动作三种行为。
/// 现在规则只在 [`DeviceReceivePolicy::for_trust_level`] 一处，三端各经自己的 binding 调它。
///
/// `previous` 传该设备**当前**的策略，用户显式设过的保存位置与代收授权会被带过去
/// （`blocked` 除外）。新配对或不关心时传 `undefined`。
#[wasm_bindgen]
pub fn default_receive_policy(
    trust_level: DeviceTrustLevelJs,
    previous: Option<DeviceReceivePolicyJs>,
) -> Result<DeviceReceivePolicyJs, JsValue> {
    let trust_level: DeviceTrustLevel = from_js_typed(trust_level.into(), "信任级别")?;
    let previous: Option<DeviceReceivePolicy> = previous
        .map(|value| from_js_typed(value.into(), "收件策略"))
        .transpose()?;
    let policy = DeviceReceivePolicy::for_trust_level(trust_level, previous.as_ref());
    to_js_typed(&policy, "收件策略")
}

#[wasm_bindgen]
pub fn inbox_search_limit() -> usize {
    swarmdrop_transfer::inbox::INBOX_SEARCH_LIMIT
}

/// 浏览器传输端节点。
#[wasm_bindgen]
pub struct WebNode {
    endpoint: Endpoint,
    /// transfer 控制面（send / offers / accept / resume）——从 [`NetManager`] 取出的 Arc。
    manager: Arc<TransferManager>,
    /// core 网络管理器：pairing（invite 配对）+ devices + shutdown（含 cleanup task 生命周期）。
    net_manager: NetManager<TransferManager>,
    /// NetManager 侧事件总线（`spawn` 注入 core 的那一个 Arc）。改名编排要往它发
    /// [`CoreEvent::DeviceRenamed`](swarmdrop_core::host::CoreEvent::DeviceRenamed)，
    /// 故这里留一份句柄——它是本 crate 唯一从 `WebNode` 直接发 core 事件的地方。
    event_bus: Arc<dyn EventBus>,
    /// 本机私钥（`generate_invite` 签名邀请用；start_node 吃的是它的 clone）。
    secret: SecretKey,
    /// 入站配对请求队列（browser-as-inviter：桌面消费本机 invite 后本机弹确认）。
    pending_pairings: PendingPairings,
    /// 已配对设备列表的持久化端口（IndexedDB 整份快照）。
    ///
    /// **配对与 identify 刷新的写入不在这里** —— 那两条都由 core 的
    /// `PairingManager::commit_paired_device` 落盘（三端同一入口）。本 crate 只在
    /// `set_receive_policy` 用它，且走 core 的列表算法，**Web 不自带一份**。
    paired_store: Arc<dyn PairedDeviceStore>,
    /// 传输域持久化端口（IndexedDB 写穿）——传输历史与**收件箱**的查询、清空都经此。
    /// 类型是 `dyn WebStore`（= `TransferStore` + 收件箱批量读）而非具体实现：宿主注入
    /// 的东西宿主自己也只该按端口用，否则「换一个存储实现」这件事在 Web 侧就少换一半。
    session_store: Arc<dyn WebStore>,
    file_access: Arc<OpfsFileAccess>,
    events_rx: RefCell<
        Option<
            futures::channel::mpsc::UnboundedReceiver<swarmdrop_transfer::events::TransferEvent>,
        >,
    >,
    /// 上一次转发中取不到、因而被跳过的 OPFS 路径，等 UI 来取（见 `send_inbox_files`）。
    skipped_forward_paths: RefCell<Vec<String>>,
}

#[wasm_bindgen]
impl WebNode {
    /// 建节点：持久化身份（Window=localStorage / Worker=OPFS）+ IndexedDB 恢复已配对设备 → 包 core 组合根 [`start_node`]
    /// （Browser [`EndpointProfile`] + Web 端口）→ 完整 [`NetManager`] + 3 协议 Router（含
    /// pairing）。**须在主线程 Window 跑**——webrtc-websys dial 碰 window，Worker 里会 panic。
    pub async fn spawn() -> Result<WebNode, JsValue> {
        // secure context 预警：非 https/localhost 源下 navigator.storage 与 crypto.subtle 缺失，
        // 接收方落盘会失败（现已快速报错而非挂死）。启动即显式提示，别等传到一半才发现。
        if !crate::env::is_secure_context() {
            tracing::warn!(
                "⚠ 当前非 secure context：navigator.storage/crypto.subtle 不可用，接收落盘会失败。\
                 请用 https 或 localhost / 127.0.0.1 访问（勿用 http 私网 IP）。"
            );
        }

        let secret = identity::load_or_create().await?;
        // web_os_info() 自建（wasm 下 OsInfo::default() 的 env 探测恒 unknown）。agent_version 由
        // start_node 走 to_agent_version()（"swarmdrop/{ver}; os=…" 契约）——桌面 DeviceManager
        // 用 AGENT_PREFIX 过滤设备列表，前缀不符会让 Web 节点在对端设备列表里隐身。
        //
        // 这里只给平台探测部分（os / platform / hostname）：`name` 由 `start_node` 从
        // [`IdbDeviceConfig`] 端口填，本机 OsInfo 于是只有组合根一个装配点。
        let os_info = web_os_info();
        // 已配对设备的事实源是这个端口——`start_node` 自己 load，这里不再预加载快照。
        let paired_store: Arc<dyn PairedDeviceStore> = Arc::new(WebPairedDeviceStore);

        // Web 端口：IndexedDB 写穿 store / OPFS 落盘 / WebEventSink（transfer 事件直连 events() 流）。
        let file_access_impl = Arc::new(OpfsFileAccess::new());
        let file_access: Arc<dyn FileAccess> = file_access_impl.clone();
        let (sink, events_rx) = WebEventSink::new();
        let transfer_events: Arc<dyn TransferEventSink> = Arc::new(sink);
        // 一个实例、两个端口视图：注入 core 的是纯端口 `TransferStore`（core 不该知道
        // Web 多出一条批量读），本节点自己持的是 `WebStore`（端口 + 那条批量读）。
        // 两者由**同一个** `Arc` 强转而来，不是两份影子副本。
        let store_impl = Arc::new(WebTransferStore::load().await);
        let store_port: Arc<dyn TransferStore> = store_impl.clone();
        let session_store: Arc<dyn WebStore> = store_impl;
        // NetManager 侧事件的 bus：捕获入站配对请求（browser-as-inviter），其余记日志。
        // transfer 事件不经此（走 WebEventSink → events() 流）。
        let (event_bus_impl, pending_pairings) = WebEventBus::new();
        let event_bus: Arc<dyn EventBus> = Arc::new(event_bus_impl);

        let file_access_for_factory = file_access.clone();
        let store_for_factory = store_port.clone();
        let events_for_factory = transfer_events.clone();
        let started = start_node(
            NodeCredentials {
                secret_key: secret.clone(),
                // 浏览器只拨号，两份服务端证书都无处可存 —— 它那侧的 WebTransport 启用
                // 判据是「有没有 `WebTransport` API」，与这两项无关。
                webrtc_certificate_pem: None,
                webtransport: None,
            },
            os_info,
            // `bootstrap_nodes` 留空：浏览器拨不了 TCP/QUIC 内置 bootstrap。公网入口
            // 由前端的 `ensureConfiguredRelays` 用 webrtc-direct 地址经 `addInfraNode`
            // 补，走的是候选表而不是这份启动配置。
            NetworkRuntimeConfig::default(),
            EndpointProfile::Browser,
            HostPorts {
                // 设备名的事实源：IndexedDB 的 `kv` store。`start_node` 自己 load 并填进
                // `OsInfo.name`，浏览器侧不再有第二份本机 OsInfo 副本。
                device_config: Arc::new(crate::device_config::IdbDeviceConfig),
                paired_device_store: paired_store.clone(),
                event_bus: event_bus.clone(),
                notifier: None, // 浏览器无系统通知
                // 邀请注册表落盘：刷新页面比重启桌面 App 频繁得多，内存态会让刚发出的邀请
                // 立刻变成「不认识」（openspec: invite-persistence）。
                invite_store: Arc::new(crate::invite_store::IdbInviteStore),
            },
            move |endpoint| {
                TransferManager::new(
                    endpoint,
                    events_for_factory,
                    store_for_factory,
                    file_access_for_factory,
                )
            },
        )
        .await
        .map_err(WebError::from)?;

        // 启动清理：上次会话遗留的 active 接收会话经状态机统一转 recoverable
        // suspended(AppRestarted)——与桌面 `cleanup_stale_sessions`、移动
        // `reconcile_stale_sessions` 同一条路径。漏做会让活动视图出现「永远在传」的幽灵条目。
        match TransferCoordinator::new(store_port, transfer_events)
            .cleanup_recoverable_sessions()
            .await
        {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!("启动清理: {n} 个遗留 active 会话转为 suspended(app_restarted)")
            }
            Err(e) => tracing::warn!("启动清理失败（历史会话状态可能不准）: {e}"),
        }

        // 过期回收**必须排在清理之后**：上一轮被强杀留下的 `Active` 会话要先转成
        // `Suspended`，`phase = Suspended` 这条判据才看得见它们（顺序反了就会整类漏掉，
        // Web 曾因回收跑在节点起来之前而不得不把判据改成「非终态」，与桌面分叉）。
        //
        // **只做 DB 侧回收，不调 `cleanup_expired_part_files`**：那个原语按会话的**全部**
        // 文件元数据重建 sink 再 `cleanup_sink`，而 Web 的 sink 路径就是文件的最终路径
        // （没有 `.part` 中间态，见 `file_access.rs` 模块注释）——一个「A 已写完、B 只写了
        // 一半」的多文件会话会把 A 一起删掉。桌面能这么做是因为它删的是 `xxx.part`。
        // OPFS 残件的清理要按「哪些文件真没写完」来做，与收件箱的文件生命周期一并处理。
        match session_store
            .reap_expired_suspended_receives(SUSPENDED_RECEIVE_RETENTION_SECS)
            .await
        {
            Ok(reaped) if reaped.is_empty() => {}
            Ok(reaped) => tracing::info!("启动清理: {} 个过期接收会话已回收", reaped.len()),
            Err(e) => tracing::warn!("过期接收会话回收失败: {e}"),
        }

        let endpoint = started.endpoint.clone();
        let net_manager = started.manager;
        let manager = net_manager.transfer_arc();
        // 事件循环（presence / infra / 状态刷新）随 router 同生命周期，n0-future spawn（wasm 友好）。
        spawn_event_loop(
            started.events,
            net_manager.shared_refs(),
            event_bus.clone(),
            started.router,
        );

        Ok(WebNode {
            endpoint,
            manager,
            net_manager,
            event_bus,
            secret,
            pending_pairings,
            paired_store,
            session_store,
            file_access: file_access_impl,
            events_rx: RefCell::new(Some(events_rx)),
            skipped_forward_paths: RefCell::new(Vec::new()),
        })
    }

    /// 本节点身份（base58）。
    pub fn node_id(&self) -> String {
        self.endpoint.node_id().to_string()
    }

    /// 当前已连接的 **SwarmDrop 客户端**数。
    ///
    /// 与桌面/移动的 `NetworkStatus.connected_peers` **走同一个函数**
    /// （`crates/core/src/network/manager.rs` 也是 `self.devices.connected_count()`），
    /// 所以三端这个数的口径天然一致。Web 此前没有这个绑定，设置页只能拿「已配对设备里
    /// 在线的台数」凑数——那是 presence 快照，未配对的对端不在里面。
    ///
    /// **不要改成读 `Endpoint::watch_conns()` 的长度。** 那是原始连接表，
    /// `publish_conns` 对每个 `ConnectionEstablished` 都建条目，不区分对端类型；
    /// 而浏览器启动时必然会连上至少一条 relay（`ensureConfiguredRelays`，那是公网可达的
    /// 前提），于是空载稳态就会显示「已连接 1 · 已配对 0」——一台设备都没配对却说连着一个。
    /// `connected_count()` 过滤 `is_swarmdrop_agent`，而 bootstrap/relay 的
    /// `agent_version` 是 `swarm-bootstrap/` 前缀（`crates/host/src/device.rs`），
    /// 正好被排除。代价是它依赖 identify 完成，比原始连接表晚约一个 RTT——桌面同此。
    ///
    /// **不一并导出 NAT 状态**：`Endpoint::watch_nat()` 的唯一写入点是 autonat 事件，
    /// 而 autonat 是 native-only（见 `crates/net/src/actor.rs` 的 `WatchSenders::nat`，
    /// 那里挂着 `cfg_attr(wasm_browser, expect(dead_code))`），wasm 下它恒为 `Unknown`。
    /// 导出一个永远不变的常量只是给界面添一行假状态；浏览器版的「别人能不能拨到我」
    /// 由 circuit 预留回答，那条已经有了（`infra_links`）。
    pub fn connected_peers(&self) -> usize {
        self.net_manager.devices().connected_count()
    }

    /// 改本机设备名：落盘 → 本机 `OsInfo` → identify 的 `agent_version` → 发
    /// `DeviceRenamed`（编排在 core 的 `device_name::rename_device`，三端同一份）。
    ///
    /// **已连接的对端一个 RTT 内就看到新名字**：新值逐连接下发给每条已建立连接的
    /// identify handler，再向这些对端主动 push；未连接的对端下次连上时直接取到新值。
    /// 节点不重启、连接不断、传输不中断，页面也不必刷新。
    ///
    /// 返回归一化后的名字（`undefined` = 已清空，对外回落到
    /// [`default_device_name`](crate::device_config::default_device_name)）。入参经
    /// `DeviceName::parse` 归一化（trim、剥控制字符与 `;`、截断到 40 个 char），所以返回值
    /// 可能与传进来的不同——UI 要展示的是这个返回值，而不是用户的草稿。
    ///
    /// 与模块级 [`set_device_name`](crate::device_config::set_device_name) 的分工：那个只
    /// 落盘，供节点起不来时的设置页用；节点在跑就走这里。两者的分支在 JS 侧
    /// （`node-runtime.ts`）——节点句柄只活在那边，Rust 够不到。
    pub async fn rename_device(&self, name: Option<String>) -> Result<Option<String>, JsValue> {
        let parsed = name.as_deref().and_then(DeviceName::parse);
        swarmdrop_core::device_name::rename_device(
            parsed.clone(),
            &crate::device_config::IdbDeviceConfig,
            &*self.event_bus,
            Some(&self.net_manager),
        )
        .await
        .map_err(WebError::from)?;
        Ok(parsed.map(DeviceName::into_string))
    }

    /// 拨任意 multiaddr（`.../ws` 或 `.../webrtc-direct/certhash/...`，须带 `/p2p/<id>`）。
    /// 返回结构化的连接信息（`{ path: "local"|"direct"|"holePunched"|"relayed", addr }`）。
    ///
    /// `signal`（可选）：标准 `AbortSignal`——超时组合用平台原语表达
    /// （`AbortSignal.timeout(5000)` / `AbortSignal.any([...])`）。abort 时 Promise
    /// 立即以 `{ kind: "aborted" }` reject；**abort ≠ 撤回拨号**（在途拨号继续到
    /// 自然失败，无常驻意图残留）。不传 signal 时由内核兜底超时（Browser 15s）
    /// 保证有限时间内 settle。
    ///
    /// ⚠️ **不要拿它判定引导节点或中继的可达性。** 三条理由：它会把候选地址**永久**写进
    /// 地址簿且没有失败回滚；对**已连接**的对端它直接返回既有连接快照，于是对已经连上的
    /// 内置节点永远返回成功——一个不可能失败的测试比没有测试更坏；而且它测的是直连链路，
    /// 中继的实际用法是 reservation，两条链路不同。可达性看
    /// [`infra_links`](Self::infra_links) 里那条关系的状态。
    pub async fn connect(
        &self,
        addr: String,
        signal: Option<web_sys::AbortSignal>,
    ) -> Result<ConnectionJsonJs, JsValue> {
        let (id, addr) = split_p2p_addr(&addr)?;
        let connect = self
            .endpoint
            .connect_with_timeout(NodeAddr::with_addrs(id, vec![addr]), REACHABILITY_TIMEOUT);
        let info = crate::abort::race(signal, connect)
            .await
            .ok_or_else(|| JsValue::from(WebError::aborted("connect 已取消")))?
            .map_err(js_err)?;
        to_js_typed(
            &ConnectionJson {
                path: info.path.into(),
                addr: info.addr.to_string(),
            },
            "连接信息",
        )
    }

    /// 解码并验签邀请串，返回对端展示信息 —— **不发起配对、不消费**。
    ///
    /// 供受邀方在粘贴 / 点链接进来之后先亮一张确认卡：篡改、伪造、格式不认的邀请在这里
    /// 就被拒掉，用户点「配对」才走 [`connect_invite`](Self::connect_invite)。
    ///
    /// **纯本地**：不拨号、不查 DHT、不碰 IndexedDB，全程零出网 —— 确认卡出现之前不该有
    /// 任何网络行为，这条是它成立的依据。
    ///
    /// **判不出「已撤销」**：撤销状态只在邀请方的注册表里，受邀方手上只有一段自包含的
    /// 签名串，那件事根本没传播过来。要判就得出网，与上一条冲突。所以撤销只能在
    /// `connect_invite` 阶段由邀请方拒绝，调用方把那个失败渲染成人话即可 ——
    /// **不要在本地发明撤销判据**（最容易发明的「查 `list_invites` 看在不在」尤其错：
    /// 那是本机自己发出的邀请，对受邀方永远为空，于是所有邀请都会被判成已撤销）。
    ///
    /// 同步返回：与 [`invite_qr_svg`](Self::invite_qr_svg) 一样是纯计算，`&self` 只是
    /// 可达性的代价（前端拿模块句柄的唯一路径是 `getNode()`）。
    pub fn decode_invite_preview(&self, invite: String) -> Result<PairInvitePreviewJs, JsValue> {
        // **分类成 invalidInput，不是 network。**「链接不对」与「网络错误」是两码事，
        // 顶着网络错误的标题只会把用户和排查都引到完全无关的方向（桌面在
        // `decode_pair_invite` 收尾时正是为这个改过分类）。技术细节只进日志。
        let invite = PairInvite::decode(&invite).map_err(|e| {
            tracing::debug!("decode_invite_preview 失败: {e}");
            JsValue::from(WebError::invalid_input(invite_parse_message(&e)))
        })?;
        let preview = PairInvitePreviewJson {
            peer_id: invite.inviter.id.to_string(),
            display_name: invite.display_name,
            display_platform: invite.display_platform,
            expires_at: invite.expires_at.to_string(),
            local_only: matches!(invite.transport_policy, TransportPolicy::LocalOnly),
        };
        to_js_typed(&preview, "邀请预览")
    }

    /// 受邀方：消费邀请串完成**真配对握手**。
    ///
    /// `pair_with_invite` 解码验签 → TTL 预检 → 按 `TransportPolicy` 过滤地址 → 连邀请方出示
    /// capability（`PairingMethod::Invite`）→ 邀请方（桌面）校验 CAS 一次性消费 + 用户确认 →
    /// 双方写配对记录。身份 pin 由握手强制（连到的必然是 `inviter_id`）。确认发生在**邀请方**
    /// 侧，浏览器侧无需交互。配对后该对端进入本机信任表，双向传输（收 / 发）不再被
    /// `NotPaired` 拦。
    ///
    /// 返回 [`PairingOutcomeJson`]：`refused` 非空表示对方拒绝了（**不是错误**），
    /// 否则 `peerId` 是已配对对端的 NodeId，`persisted` 为 `false` 时表示配对成功了但没写进
    /// IndexedDB —— 刷新页面后这台设备会不见（对端仍记着）。
    pub async fn connect_invite(&self, invite: String) -> Result<PairingOutcomeJs, JsValue> {
        let (response, paired) = self
            .net_manager
            .pairing()
            .pair_with_invite(&invite)
            .await
            .map_err(WebError::from)?;

        // `paired.is_some()` 与「响应是 Success」是同一件事 —— core 只在成功那一臂构造
        // commit（`PairingManager::request_pairing`），所以拒绝原因只能从 response 取。
        //
        // **不再包成 `WebError`。** 「对方点了拒绝」是一次完全正常的交互，压成 `network`
        // kind 会让用户看到「网络错误」加一句写死的简体中文；桌面 `pairing-store.ts` 一直
        // 是按 reason 出文案的，Web 这条分叉纯属遗漏。
        let commit = match paired {
            Some(commit) => commit,
            None => {
                // 穷尽 match：内核加一个拒绝原因，这里编译失败 —— 那正是本地投影
                // `PairingRefusedJson` 的安全性来源（见其文档）。
                let refused = Some(match response {
                    PairingResponse::Refused {
                        reason: PairingRefuseReason::UserRejected,
                    } => PairingRefusedJson::UserRejected,
                    // 走不到：core 在 Success 那一臂必构造 commit。真到了这里说明上游变了，
                    // 按「对方拒绝」处理是最不误导的降级。
                    PairingResponse::Success => {
                        tracing::warn!("配对响应为 Success 但没有 commit，按拒绝处理");
                        PairingRefusedJson::UserRejected
                    }
                });
                return to_js_typed(
                    &PairingOutcomeJson {
                        refused,
                        peer_id: String::new(),
                        persisted: true,
                    },
                    "配对结果",
                );
            }
        };

        // 落盘 / 共享内存表 / `PairedDeviceAdded` 事件都已由 core 的
        // `commit_paired_device` 一并完成（三端同一个入口），这里不再自己 upsert。
        // 顺带解决了「拿到的 `device` 恒是默认策略的 `PairedDeviceInfo::new`」那个坑：
        // commit 写回内存表的是**合并后**的版本，用户设过的 trust_level /
        // receive_policy 不会被静默重置。
        to_js_typed(
            &PairingOutcomeJson {
                refused: None,
                peer_id: commit.device.peer_id.to_string(),
                persisted: commit.persisted,
            },
            "配对结果",
        )
    }

    /// 发起方（browser-as-inviter）：生成一次性签名邀请串，供桌面/移动扫码或粘贴消费。
    ///
    /// `local_only=true` 走 LocalOnly（受邀方只用私网地址）。邀请自包含本机 dialable 地址提示——
    /// 浏览器不 listen 本地 socket，其可达地址来自 **relay reservation**（circuit 地址）；故桌面要
    /// 拨得到本机，本机需先经 [`infra_ensure`](Self::infra_ensure) 在某引导节点上建 reservation
    /// （等到 `active`），否则邀请里无可拨地址、消费方连不上。
    /// **async 化于 invite-persistence**：生成时要把邀请写穿进 IndexedDB，否则刷新页面
    /// 后本机就不认识刚发出去的那条邀请了（注册表 fail-closed，查不到即拒绝）。
    pub async fn generate_invite(&self, local_only: bool) -> Result<String, JsValue> {
        let policy = if local_only {
            TransportPolicy::LocalOnly
        } else {
            TransportPolicy::Auto
        };
        Ok(self
            .net_manager
            .pairing()
            .encode_invite(&self.secret, policy)
            .await)
    }

    /// 邀请二维码的 SVG 字符串（深模块 + 透明背景，渲染端自己套白卡）。
    ///
    /// 编码规范由 [`swarmdrop_invite::qr`] 三端单点固化（原样编码 + 最优分段 + ECL::M +
    /// quiet zone）——浏览器**不要**另引 JS 二维码库：三端各画一遍，码面规范就会漂，
    /// 而漂了的表现是「某一端生成的码另一端扫不出来」，很难归因。
    ///
    /// **这是纯函数，`&self` 只是可达性的代价，不代表它是节点能力**——别把这里当作
    /// 「纯计算也该挂 `WebNode`」的先例。做成自由函数或 `WebNode` 的静态方法都更贴切，
    /// 但前端拿 wasm 模块句柄的唯一路径是 `node-runtime.ts` 里那个**不导出**的
    /// `loadModule()`（静态 import 会在 Next 预渲染时挂，故只能动态 import + 记忆化）。
    /// 走自由函数就得再开一个 `getModule()` 访问器并自己缓存一份——为一个叶子功能
    /// 加这套机器不值，而 `getNode()` 是现成的。
    ///
    /// 同步返回：纯计算，不碰 IndexedDB 也不碰网络。
    pub fn invite_qr_svg(&self, invite: String) -> Result<String, JsValue> {
        // 不用 `js_err`（它一律映射成 `kind: "network"`）：编码失败是纯计算的「输入装不下」
        // ——QR 在 ECL::M 下的容量上限约 2KB wire，而本机产出的邀请最坏 327 字节，够不着。
        // 真报出来时若顶着「网络错误」的标题，只会把排查引到完全无关的方向。
        swarmdrop_invite::invite_qr_svg(&invite)
            .map_err(|e| WebError::invalid_input(e.to_string()).into())
    }

    /// 撤销本机发出的邀请（重新生成覆盖旧串、用户放弃、关闭邀请界面）。
    ///
    /// 幂等且不报错——不认识的串直接 no-op（详见 `PairingManager::revoke_invite`），
    /// 调用方 fire-and-forget 即可（**返回 Promise，不 await 也能用**）。
    ///
    /// async 化于 invite-persistence：撤销要把那行从 IndexedDB 删掉，否则刷新后它又回来了。
    /// 返回**是否已落盘**：`false` 时重启后那条邀请会复活，调用方应当提示用户。
    pub async fn revoke_invite(&self, invite: String) -> bool {
        self.net_manager.pairing().revoke_invite(&invite).await
    }

    /// 本机未过期的已发出邀请（最近生成的在前）。
    ///
    /// TTL 24h + 跨刷新存活之后，「我现在有几条邀请在外面飘」需要能看见 ——
    /// 这个列表与 [`revoke_invite_by_id`](Self::revoke_invite_by_id) 是那段窗口的控制手段。
    pub fn list_invites(&self) -> Result<InviteListArray, JsValue> {
        let items: Vec<InviteListItemJson> = self
            .net_manager
            .pairing()
            .list_invites()
            .into_iter()
            .map(|summary| InviteListItemJson {
                id: capability_hash_to_hex(&summary.capability_hash),
                created_at: summary.created_at.to_string(),
                expires_at: summary.expires_at.to_string(),
                consumed: summary.consumed,
            })
            .collect();
        to_js_typed(&items, "邀请列表")
    }

    /// 按列表条目的 `id`（capability 哈希 hex）撤销 —— 列表里没有原始邀请串。
    /// 返回**是否已落盘**：`false` 时刷新后那条邀请会复活，调用方应当提示用户。
    pub async fn revoke_invite_by_id(&self, id: String) -> Result<bool, JsValue> {
        // 入参格式错不是网络错误 —— `kind` 是前端渲染文案的判别码，报 `network` 会让用户
        // 看到「网络连接出现问题，请稍后重试」。桌面/移动同一处用的是 `InvalidArgument`。
        let hash = capability_hash_from_hex(&id)
            .ok_or_else(|| JsValue::from(WebError::invalid_input("邀请标识格式非法")))?;
        Ok(self.net_manager.pairing().revoke_invite_by_hash(hash).await)
    }

    /// 挂起的入站配对请求（消费方扫/粘本机 invite 后到达）。**取出即清空**，调用方自行累积展示。
    pub fn pending_pairing_requests(&self) -> Result<PendingPairingArray, JsValue> {
        let items: Vec<PendingPairingJson> = self
            .pending_pairings
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default();
        to_js_typed(&items, "配对请求")
    }

    /// 响应一个入站配对请求（`accept=true` 接受并写配对记录、CAS 消费 invite / `false` 拒绝）。
    pub async fn respond_pairing_request(
        &self,
        pending_id: String,
        accept: bool,
    ) -> Result<bool, JsValue> {
        let id: u64 = pending_id
            .parse()
            .map_err(|_| WebError::invalid_input("无效的 pending_id"))?;
        let response = if accept {
            PairingResponse::Success
        } else {
            PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            }
        };
        let paired = self
            .net_manager
            .pairing()
            .respond_pairing_request(id, response)
            .await
            .map_err(WebError::from)?;
        // 同 `connect_invite`：落盘与事件都在 core 的 `commit_paired_device` 里。
        // 返回是否已落盘（`false` = 刷新后这台设备会不见，对端仍记着）；
        // 「没配成时报 true」那条约定的判据在 core，三端共用。
        Ok(swarmdrop_core::pairing::persisted_or_absent(
            paired.as_ref(),
        ))
    }

    /// 解除与某台已配对设备的配对（`peer_id` 为 base58 NodeId）。
    ///
    /// 走 core 的 `PairingManager::unpair`：**先落盘、再删共享内存表、最后发事件**。
    /// 持久化失败即整体报错且内存表不动——绝不出现「这次点了就没了、刷新一下又回来」。
    /// 删内存表这一步同时撤销 presence 保活与 `is_paired` 判定（一个 tick 内收敛），
    /// 所以本方法之后不需要再补任何本地清理。
    ///
    /// **单方语义**：只解除本机这一侧，对端仍然认得本机；对端再发起传输会被 `NotPaired`
    /// 拒掉，要恢复得重新走一次完整配对。
    ///
    /// 幂等：本来就没配对的 peer 直接返回成功，不发事件。
    pub async fn remove_paired_device(&self, peer_id: String) -> Result<(), JsValue> {
        let id = parse_node_id(&peer_id)?;
        self.net_manager
            .pairing()
            .unpair(&id)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 更新已配对设备的信任级别与收件策略。
    ///
    /// 与桌面 `update_paired_device_policy` 命令**同一条路径**：落盘与「节点在跑时把新值推进
    /// 共享内存表」都在 core 的
    /// [`set_receive_policy`](swarmdrop_core::paired_devices::set_receive_policy)。
    /// 后半句不能省——`swarmdrop_transfer::policy` 裁决入站 offer 时读的是内存表那份，
    /// 只落盘会变成「策略已保存、本次运行仍按旧策略放行」。存在性检查也只在那一处。
    ///
    /// `receive_policy` 传 `undefined` 表示**按新信任级别取默认策略**（`for_trust_level`），
    /// 这是「只改信任级别、策略跟着走」那条路径；传具体值则逐字段采用。
    ///
    /// **返回 `()`，调用方自己重取一次 `paired_devices()`**——与
    /// [`remove_paired_device`](Self::remove_paired_device) 同一个约定。两个理由：
    /// core 这条路径不发事件（没有对应的 `CoreEvent` 变体，补一条会波及三端全部 event
    /// adapter 的穷尽 match，是独立增量）；而 `paired_devices()` 在 Web 侧是同步的内存查询，
    /// 重取一次比把 `PairedDeviceInfo`（存储型）也搬进 Web 的类型面便宜——那一面目前只有
    /// `Device` 这一个读模型，多一个就多一处要解释「这两个有什么区别」。
    pub async fn update_paired_device_policy(
        &self,
        peer_id: String,
        trust_level: DeviceTrustLevelJs,
        receive_policy: Option<DeviceReceivePolicyJs>,
    ) -> Result<(), JsValue> {
        let id = parse_node_id(&peer_id)?;
        let trust_level: DeviceTrustLevel = from_js_typed(trust_level.into(), "信任级别")?;
        let receive_policy: Option<DeviceReceivePolicy> = receive_policy
            .map(|value| from_js_typed(value.into(), "收件策略"))
            .transpose()?;

        swarmdrop_core::paired_devices::set_receive_policy(
            &id,
            trust_level,
            receive_policy,
            &*self.paired_store,
            Some(&self.net_manager),
        )
        .await
        .map_err(WebError::from)?;
        Ok(())
    }

    /// 已配对设备清单——与桌面 `list_devices` 同源的 [`DeviceManager::get_devices`] 读模型
    /// （含在线状态/连接类型，presence 在 Web 侧同样运作）。
    pub fn paired_devices(&self) -> Result<DeviceArray, JsValue> {
        let devices = self.net_manager.devices().get_devices(DeviceFilter::Paired);
        to_js_typed(&devices, "已配对设备")
    }

    // ── 基础设施意图（声明式集合，替代一次性 RPC 形态的 reserve()）──
    //
    // reservation 是「持续维持的可达状态」而非「一次完成的动作」：命令（ensure/
    // drop）改期望状态、同步返回；实际状态经查询（infra_links）与订阅
    // （infra_changed）到达。意图生命周期与单次等待的耐心（AbortSignal）解耦。
    //
    // **命名从 `relays_*` 改成 `infra_*`**：登记的是一段「基础设施关系」，relay 只是它
    // 可能承担的角色之一（另一个是 kad 路由种子）。两个角色在内核里从第一天起就正交，
    // 是本仓自建的那台恰好兼任，才让上层一路把它当成一个东西。

    /// 登记一个基础设施节点的常驻意图（校验 + 登记，同步返回）。
    ///
    /// 浏览器被动接收连接的唯一入口。拨号 / reservation / 断线重建由 core 的
    /// InfraSupervisor 统一收敛（最迟 1s 内启动第一轮，失败退避重试）；进度经
    /// [`infra_links`](Self::infra_links) / [`infra_changed`](Self::infra_changed)
    /// 观测，或用 [`infra_until_active`](Self::infra_until_active) 等首次建立。
    ///
    /// **校验走 core 的 `add_infra_node`，前端不重写一份规则。** 三条判据里有两条要
    /// 认识内核事实（合法 peer id 形状、本端点**实际装配了哪些 transport**），后者正是
    /// 浏览器最容易踩的——粘一条 `/tcp/` 进来今天会被静静收下，然后永远连不上且毫无提示。
    /// 失败时 reject 一个 `InfraAddrError`（`{ kind, … }`，形状见 bindings.ts），
    /// **不是** `WebError`：它要回答的是「这条地址哪里不对」，而不是「哪一层出了错」。
    ///
    /// **`Duplicate` 也照常 reject。** 它曾被这里吞成成功，理由是「回放要幂等」——不成立：
    /// 回放（`replayInfraNodes`）本来就 try/catch 且只 `console.error`，而且它跑在一张空的
    /// 候选表上，压根产不出重复。代价却是实打实的：用户粘一条已在清单里的地址会看到
    /// 「已添加引导节点，正在连接…」而其实什么都没发生，`duplicate` 那句文案成了死代码。
    /// 登记的**效果**仍然幂等（core 的 upsert 会合并），幂等的是状态不是回执。
    ///
    /// 全部规则零网络往返。「它到底连不连得上」由提交后的收敛环回答——那测的才是后续
    /// 真正会走的那条链路（旧的「测试连通性」按钮走直连，对已连上的节点永远绿）。
    ///
    /// 返回节点的 base58 NodeId——即 `infra_drop` / `infra_until_active` 的入参，
    /// 调用方直接串联，无需自行解析 multiaddr 的 `/p2p/` 段。
    pub fn infra_ensure(&self, addr: String) -> Result<String, JsValue> {
        // 角色给全（kad + relay）：浏览器同样要靠它进 kad 路由表做 DHT 查询。
        // 此前只给 relay 角色，kad 那半是靠 `learn_candidate` 在 identify 之后补回来的
        // ——巧合，不是设计。
        match self
            .net_manager
            .add_infra_node(&addr, CandidateRoles::kad_and_relay())
        {
            Ok(peer) => Ok(peer.id.to_string()),
            // `Duplicate` 不再单独截胡（理由见上），与其余判别码同路 reject。
            Err(e) => Err(infra_addr_err(e)),
        }
    }

    /// 撤销基础设施意图（[`infra_ensure`](Self::infra_ensure) 的对称面）。
    ///
    /// **真撤销**而非停止等待：停止后台收敛重试、关闭 circuit listener、立刻
    /// 断开与该节点的连接（含中止在途拨号），条目从状态集合消失。
    pub async fn infra_drop(&self, peer_id: String) -> Result<(), JsValue> {
        let id = parse_node_id(&peer_id)?;
        self.net_manager
            .remove_infra_intent(id)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 全量基础设施关系快照（[`InfraLink`](swarmdrop_core::infra::InfraLink)`[]`）。
    ///
    /// 每条同时带**意图侧**（地址 / 来源 / 角色 / scope / 首末次见到 / 能否移除）与
    /// **观测侧**（是否已连、relay 轨道状态与失败原文）。零存储读模型，现场 join
    /// 候选表与内核两条 watch——所以「状态粘死」在物理上不可能发生。
    pub fn infra_links(&self) -> Result<InfraLinkArray, JsValue> {
        to_js_typed(
            &swarmdrop_core::infra::build_infra_links(&self.net_manager.shared_refs()),
            "基础设施状态",
        )
    }

    /// 基础设施状态变化流：每次变化产出一份全量快照（可直接 setState）。
    /// 可多次调用（每次独立订阅），与 `events()` 的单点消费不同。
    ///
    /// **触发源是 `watch_relays`**：内核不外露候选表与在途拨号的变化，而 relay 轨道的
    /// 每一次翻转（Connecting / Active / Failed）都从那里出。意图侧的增删由调用方自己
    /// 知道（它就是发起方），补一次 `infra_links()` 即可。
    pub fn infra_changed(&self) -> InfraChangedStream {
        let watcher = self.endpoint.watch_relays();
        let shared = self.net_manager.shared_refs();
        let stream = futures::stream::unfold((watcher, shared), |(mut w, shared)| async move {
            w.updated().await?;
            let links = swarmdrop_core::infra::build_infra_links(&shared);
            let value = crate::serialize::to_js(&links).unwrap_or(JsValue::NULL);
            Some((Ok::<JsValue, JsValue>(value), (w, shared)))
        });
        JsValue::from(wasm_streams::ReadableStream::from_stream(stream).into_raw()).unchecked_into()
    }

    /// 等待某条关系的 relay 首次进入 `active`，resolve 出 circuit 可达地址（内核拼装）。
    ///
    /// 观察到 `failed` 时**立即 reject**（把「要不要再等下一轮退避」还给调用方），
    /// 意图保留——要停止后台收敛请调 [`infra_drop`](Self::infra_drop)。
    /// `signal`（可选）：abort 只是不再等待，同样不改变意图生命周期。
    /// 不传 signal 时 30s 兜底超时保证 Promise 有限时间内 settle。
    pub async fn infra_until_active(
        &self,
        peer_id: String,
        signal: Option<web_sys::AbortSignal>,
    ) -> Result<String, JsValue> {
        let id = parse_node_id(&peer_id)?;
        let mut relays = self.endpoint.watch_relays();
        let wait = async move {
            // 首轮读快照，此后消费 updated() 的返回值——每次唤醒只做一份 map 拷贝
            let mut map = relays.get();
            loop {
                match map.get(&id) {
                    Some(RelayState::Active { circuit_addr }) => {
                        return Ok(circuit_addr.to_string());
                    }
                    Some(RelayState::Failed { last_error }) => {
                        return Err(WebError::network(format!("relay 建立失败: {last_error}")));
                    }
                    _ => {}
                }
                map = match relays.updated().await {
                    Some(map) => map,
                    None => return Err(WebError::network("endpoint 已关闭")),
                };
            }
        };
        let bounded = async {
            n0_future::time::timeout(UNTIL_ACTIVE_CAP, wait)
                .await
                .map_err(|_| WebError::network("等待 reservation 超时"))?
        };
        match crate::abort::race(signal, bounded).await {
            Some(result) => Ok(result?),
            None => Err(WebError::aborted("until_active 已取消（意图保留）").into()),
        }
    }

    /// 向 `to`（base58 NodeId）发送用户选择的文件：登记文件源 → prepare（checksum + bao
    /// outboard）→ 发 Offer。返回 session_id。
    pub async fn send_files(&self, to: String, files: Vec<File>) -> Result<String, JsValue> {
        if files.is_empty() {
            return Err(WebError::invalid_input("未选择文件").into());
        }
        // prepared_id 提到登记之前：它同时充当本批源 id 的命名空间。
        let prepared_id = Uuid::new_v4();

        let mut entries = Vec::with_capacity(files.len());
        let mut sources = Vec::with_capacity(files.len());
        for (idx, file) in files.iter().enumerate() {
            let name = file.name();
            // 选文件夹时浏览器把目录内路径挂在非标准属性 `webkitRelativePath` 上
            // （`<input webkitdirectory>`）。web-sys 0.3 没生成它，所以走 Reflect 直取；
            // 单选文件时它是空串，回落到文件名。
            //
            // 值是浏览器构造的、永远不含 `..` 或绝对路径；接收侧另有
            // `is_safe_relative_path` 把关，那道校验防的是**恶意对端**而不是这里。
            let relative_path =
                js_sys::Reflect::get(file, &JsValue::from_str("webkitRelativePath"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| name.clone());
            // **源 id 必须每次登记唯一，不能用文件名。** 源注册表是 map，用文件名当 key 时
            // 一次发送里挑两个同名文件（不同目录下的 `report.pdf` 很常见）后者会顶掉前者，
            // 两条 entry 于是指向同一个 `File` 却各自声明着不同的 size——`prepare` 要么报
            // 「read_source_chunk 返回长度异常」，要么**发出错误的文件内容**。
            //
            // 形如 `{prepared_id}/{idx}`：唯一、跨多次发送不冲突，且在日志与
            // `transfer_file.source_path`（续传时由 `build_prepared_files_from_db` 读回）
            // 里仍然认得出是哪一批的第几个。对端看到的路径走 `relative_path`，与 id 无关。
            let source_id = FileSourceId(format!("{prepared_id}/{idx}"));
            sources.push((source_id.clone(), relative_path.clone(), file.clone()));
            entries.push(HostEnumeratedFile {
                source_id,
                name,
                relative_path,
                size: file.size() as u64,
            });
        }
        // 整批一次登记：源表按**批**淘汰上限（半批淘汰会让一次续传读到一半才发现源没了）。
        self.file_access.register_batch(sources);

        let prepared = self
            .manager
            .prepare(prepared_id, entries)
            .await
            .map_err(WebError::from)?;
        let file_ids: Vec<u32> = prepared.files.iter().map(|f| f.file_id).collect();

        let result = self
            .manager
            .send_offer(
                &prepared_id,
                &to,
                &self.paired_device_name(&to),
                &file_ids,
                TransferOrigin::Human,
            )
            .await
            .map_err(WebError::from)?;
        Ok(result.session_id.to_string())
    }

    /// 转发已接收的文件：把 OPFS 里的条目取回成 `File`，之后与用户选文件发送**完全同路**。
    ///
    /// `paths` 是收件箱条目的 OPFS 相对路径（落盘时写的那个）。`FileSystemFileHandle::get_file()`
    /// 返回的正是 `send_files` 已经在吃的 `web_sys::File`，所以读分块那条路径一行都不用动——
    /// 转发在后端从来不缺能力，缺的只是一个入口。
    ///
    /// 拿到的 `File.name()` 是路径末段，`webkitRelativePath` 为空，于是 `relative_path` 回落
    /// 到文件名。这是要的行为：转发是一次新的发送，把上一次传输的目录结构带给第三台设备
    /// 只会让对方莫名其妙（移动端同此约定）。
    /// **取不到的条目被跳过，而不是让整批失败。** OPFS 是配额存储，条目可能被浏览器驱逐；
    /// 「一个死路径 → 整次转发失败 → 用户看到一条没有文件名的 DOMException」正是
    /// Received File Reuse Contract 里「发起前筛掉」要杜绝的。移动端由 `selectForwardable`
    /// 承担这件事，浏览器这边没有对应的 per-path 原语可用，所以筛在这里。
    ///
    /// 被跳过的路径经 [`Self::take_skipped_forward_paths`] 取回，由 UI 告诉用户。全部取不到
    /// 才算失败——那时确实没有任何东西可发。
    pub async fn send_inbox_files(
        &self,
        to: String,
        paths: Vec<String>,
    ) -> Result<String, JsValue> {
        if paths.is_empty() {
            return Err(WebError::invalid_input("未选择文件").into());
        }
        let mut files = Vec::with_capacity(paths.len());
        let mut skipped = Vec::new();
        for path in &paths {
            match crate::opfs::open_file(path).await {
                Ok(file) => files.push(file),
                Err(err) => {
                    tracing::warn!(path, error = %err, "转发跳过取不到的文件");
                    skipped.push(path.clone());
                }
            }
        }
        if files.is_empty() {
            *self.skipped_forward_paths.borrow_mut() = Vec::new();
            return Err(WebError::storage("这些文件都已不在浏览器存储里").into());
        }
        *self.skipped_forward_paths.borrow_mut() = skipped;
        self.send_files(to, files).await
    }

    /// 取回上一次转发中被跳过的路径，**取过即清**。
    ///
    /// 单独一个方法而不是塞进 `send_inbox_files` 的返回值：那个返回的是 session_id，
    /// 换成结构体会让所有既有调用点跟着改，而这条信息只有转发这一个入口关心。
    pub fn take_skipped_forward_paths(&self) -> Vec<String> {
        std::mem::take(&mut self.skipped_forward_paths.borrow_mut())
    }

    /// 发送记录里存的对端显示名。
    ///
    /// 这里曾经是字面量 `"web"`，于是**发出去的每一条记录都叫「web」**——传输页里几行除了
    /// 时间戳完全同形，同一个文件发两次根本认不出哪条是哪条（而 `peerName` 正是那一行用来
    /// 回答「发给谁」的唯一字段）。桌面与移动没有这个毛病：它们的 `start_send` /
    /// `send_prepared` 都由调用方传设备名。
    ///
    /// **在 Rust 侧查而不是给 `send_files` 加参数**：查询与规则都与接收侧同源——同一份
    /// `paired_devices` 表（接收侧经 `PeerDirectory::get_paired_device` 拿，见
    /// `TransferCtrlService`）、同一个 [`OsInfo::display_name`]。前端手上那个名字是展示层的
    /// （`organizedDeviceName` 把本机别名也算进去），那是「我怎么称呼它」，不该写进记录。
    ///
    /// 查不到时返回空串，回落交给展示层（`peerLabel` → `shortPeerId`）：写死的占位会跟着
    /// 记录一起落库，将来换措辞、换语言都改不动它。
    fn paired_device_name(&self, peer_id: &str) -> String {
        parse_node_id(peer_id)
            .ok()
            .and_then(|id| self.net_manager.pairing().get_paired_device(&id))
            .map(|device| device.os_info.display_name())
            .unwrap_or_default()
    }

    /// 当前挂起（待确认）的入站 offer 列表。
    pub fn pending_offers(&self) -> Result<OfferJsonArray, JsValue> {
        let offers: Vec<OfferJson> = self
            .manager
            .pending_offers()
            .into_iter()
            .map(|o| OfferJson {
                session_id: o.session_id.to_string(),
                peer_id: o.peer_id.to_string(),
                peer_name: o.peer_name,
                total_size: o.total_size,
                files: o.files,
            })
            .collect();
        to_js_typed(&offers, "offers")
    }

    /// 接受入站 offer 并开始接收（落 OPFS）。
    pub async fn accept_offer(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        // OPFS 落盘路径由文件 relative_path 决定，save_location 语义在 Web 侧为占位。
        let save = CoreSaveLocation::Path {
            path: "opfs".to_string(),
        };
        self.manager
            .accept_and_start_receive(&sid, save)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 拒绝入站 offer。
    pub async fn reject_offer(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .reject_and_respond(&sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 取消一条**发送**会话。
    ///
    /// 只是一条 wasm 边界上的线，取消语义整套在域层（`crates/transfer`）里做完：
    /// 按 wire 发 Cancel 帧通知对端（对方随即清掉自己的半成品）、dispatch
    /// `UserCommand::Cancel` 让协调器把会话写成**不可续传**的终态（`recoverable=false`，
    /// 故刷新后不会再冒出「续传」按钮）、并按 `session_id` 索引只动这一条会话。
    /// **Web 侧不要再补任何本地取消逻辑**，否则就有了第二条状态机路径。
    ///
    /// 覆盖「offer 已发出、对方还没接受」这条边界：此时没有 send actor，域层会回落到
    /// `outbound_offers`（`flow/send.rs`）——丢弃 prepared、照样 dispatch `Cancel`，
    /// 所以「发出去等半天对方不理」也止得住损，调用方无需区分。
    pub async fn cancel_send(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .cancel_send(&sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 取消一条**接收**会话。
    ///
    /// 与 [`cancel_send`](Self::cancel_send) 同样只是导出：域层通知对端停发、
    /// dispatch `UserCommand::Cancel` 进不可续传终态，并调 `cleanup_part_files()`
    /// 逐个走 `FileAccess::cleanup_sink` 清掉本次会话开出来的半成品——在 Web 上那就是
    /// [`OpfsFileAccess::cleanup_sink`](crate::file_access)，OPFS 里的截断文件会被真删掉。
    ///
    /// **方向不自动判**（要发送就调 `cancel_send`）：取消是有副作用的操作（发帧、删文件、
    /// 写终态），拿它当探针试方向会把「dispatch 失败」误读成「不是这个方向」。
    pub async fn cancel_receive(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .cancel_receive(&sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 暂停一条**发送**会话。
    ///
    /// 与取消同样只是一条 wasm 边界上的线：域层停掉 sender actor、把文件级进度落库、
    /// dispatch `UserCommand::Pause`（`active` → `suspended(LocalPaused)`，
    /// **`recoverable = true`**），并通知对端。之后调 [`resume`](Self::resume) 接着传。
    ///
    /// ## 浏览器上它为什么恢复得了（与「发送不跨刷新」不矛盾）
    ///
    /// [`initiate_resume`] 要的两样东西在**同一个页面生命周期内**都还在：
    ///
    /// - **会话记录**：`WebTransferStore` 是「内存读缓存 + IndexedDB 写穿」，`create_session`
    ///   无条件写内存，`worth_persisting` 只决定要不要**再**写 IndexedDB。所以非终态发送
    ///   会话查得到，只是刷新后就没了。
    /// - **文件内容**：用户选的 `File` 存在 [`OpfsFileAccess`](crate::file_access) 的源注册表
    ///   里，登记后不移除，`read_source_chunk` 照常读得到。
    ///
    /// 刷新之后两样同时消失，`initiate_resume` 在 `find_session` 那一步就报「会话不存在」
    /// ——那正是应有的行为，不需要在这里另设守卫（见 `store.rs` 的落库范围表）。
    pub async fn pause_send(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .pause_send(&sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 暂停一条**接收**会话。
    ///
    /// 与 [`pause_send`](Self::pause_send) 对称，但落盘的半成品**不清理**（那是取消才做的事）
    /// ——OPFS 里已写入的部分连同 checkpoint 一起留着，`resume` 从断点续。
    ///
    /// 接收方向的 suspended 会话 `worth_persisting`，所以它**跨刷新也能续**：重新打开页面后
    /// 会话仍在传输列表里，「续传」照常可点。
    ///
    /// **方向不自动判**，理由同 `cancel_*`：暂停有副作用（停 actor、写状态、通知对端），
    /// 拿它当探针试方向会在第一条真失败时顺手对另一个方向也来一遍。
    pub async fn pause_receive(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .pause_receive(&sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 手动发起断点续传（对某 suspended 会话）。
    ///
    /// 三种 suspended 都走这一条：用户自己暂停的（`LocalPaused`）、对端暂停的
    /// （`RemotePaused`）、以及连接中断 / 对方离线。恢复需要对端在线并应答探测，
    /// 失败时错误照常经 `WebError` 透出。
    pub async fn resume(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .initiate_resume(sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 已持久化的传输会话投影，**按 `startedAt` 倒序**（端口契约，三端一致）。
    ///
    /// 页面刷新后事件流从零开始，前端据此回补收件箱与传输活动视图（收件箱 = 其中
    /// `direction=receive` 且 `terminalReason=completed` 的条目，文件仍在 OPFS，可继续
    /// [`download_url`](Self::download_url)）。各面板再按自己的维度（结束时间 / 更新时间）
    /// 重排是预期行为——端口保证的是确定性，不是最终展示序。
    ///
    /// **不含**非终态的发送会话与待决 offer：浏览器刷新后无法在不重新选择文件的前提下
    /// 读回 `File`，待决 offer 也已无处应答，故它们本就不落库（见 `store.rs` 模块注释）。
    pub async fn transfer_history(&self) -> Result<TransferProjectionArray, JsValue> {
        let projections = self
            .session_store
            .list_transfer_projections()
            .await
            .map_err(WebError::from)?;
        to_js_typed(&projections, "传输历史")
    }

    /// 删除一条传输记录。
    ///
    /// **只删记录**：OPFS 里已落盘的文件不动，收件箱照旧能看能下载——文件的生命周期归
    /// 收件箱侧管（三端一致的分工，别在这里发明 Web 特例）。
    ///
    /// 进行中的会话会被域层拒绝（`TransferManager::delete_session` 的守卫），错误经
    /// `WebError` 透出——UI 的按钮可见性只是第一道，绕过它直调导出同样删不掉。
    pub async fn delete_transfer_session(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .delete_session(sid)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 清空传输历史：删除所有**已结束**的会话记录，进行中与已中断的一条不动。
    ///
    /// 同样只清账本，收件箱里的文件不受影响（见 [`delete_transfer_session`](Self::delete_transfer_session)）。
    pub async fn clear_transfer_history(&self) -> Result<(), JsValue> {
        self.session_store
            .clear_all_history()
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    // ── 收件箱（「已接收内容」这本账）──────────────────────────────────────
    //
    // 与 [`transfer_history`](Self::transfer_history) 刻意分开：收件箱是**结果**
    // （已落盘、可回看），传输历史是**过程**。两者是两张各自的表，清空历史与
    // `HISTORY_CAP` 淘汰都不动收件箱（见 `store.rs` / `inbox.rs` 的模块注释）。
    //
    // **刻意不导出**两个端口方法：
    // - `mark_inbox_item_file_missing`：浏览器侧没有「文件被外部移动/删除」这回事
    //   （OPFS 只有本 origin 能写）。真会发生的是**配额驱逐**，那会让整个 OPFS 目录
    //   一起消失，逐文件打 missing 标记没有意义。
    // - `repair_missing_inbox_items_for_completed_receives`：它修的是「`ensure_*` 当时
    //   写失败」，而 Web 端此刻没有承载它的 UI。端口实现照做（三端同构），导出等有
    //   调用方再加——不预留死接口。

    /// 收件箱条目列表，按 `receivedAt` 倒序；`includeArchived=false` 时排除已归档项，
    /// 软删项一律不返回。
    ///
    /// **返回的是完整详情**（含文件清单与关联传输投影），不是 summary。前端此前拿到
    /// summary 后要 `Promise.all(summaries.map(inbox_item))` 逐条补详情——1 + N 次 wasm
    /// 调用，且拉详情与拉列表之间条目可能已被删（于是要 `filter(d => d !== null)` 去兜
    /// 一个自己制造出来的竞态）。而收件箱在浏览器侧是全内存表，列表与详情读的是同一份
    /// 数据，那 N 次调用买不到任何新鲜度。
    pub async fn inbox_items(
        &self,
        include_archived: bool,
    ) -> Result<InboxItemDetailArray, JsValue> {
        let items = self.session_store.list_inbox_details(include_archived);
        to_js_typed(&items, "收件箱条目")
    }

    /// 单条收件箱详情（含文件清单与关联传输投影）；不存在或已软删返回 `null`。
    pub async fn inbox_item(&self, item_id: String) -> Result<InboxItemDetailJs, JsValue> {
        let id = parse_uuid(&item_id, "item_id")?;
        let detail = self
            .session_store
            .get_inbox_item_detail(id)
            .await
            .map_err(WebError::from)?;
        to_js_typed(&detail, "收件箱详情")
    }

    /// 按传输会话 id 取收件箱详情（「这次传输收到的东西」的反查）；无关联返回 `null`。
    pub async fn inbox_item_by_session(
        &self,
        session_id: String,
    ) -> Result<InboxItemDetailJs, JsValue> {
        let sid = parse_session_id(&session_id)?;
        let detail = self
            .session_store
            .get_inbox_item_by_transfer_session_id(sid)
            .await
            .map_err(WebError::from)?;
        to_js_typed(&detail, "收件箱详情")
    }

    /// 收件箱子串检索：大小写不敏感，覆盖标题 / 来源设备名 / 文件名与相对路径。
    /// 空查询返回空列表；结果按 `receivedAt` 倒序并截断到 `limit`
    /// （缺省取三端共享的 `INBOX_SEARCH_LIMIT`，前端不必自带魔数）。
    pub async fn search_inbox(
        &self,
        query: String,
        limit: Option<u32>,
        include_archived: bool,
    ) -> Result<InboxSearchHitArray, JsValue> {
        let hits = self
            .session_store
            .search_inbox_capped(&query, limit, include_archived)
            .await
            .map_err(WebError::from)?;
        to_js_typed(&hits, "收件箱检索结果")
    }

    /// 标记条目最近打开时间（用户点开详情/下载时调）。条目不存在时静默成功。
    pub async fn mark_inbox_item_opened(&self, item_id: String) -> Result<(), JsValue> {
        let id = parse_uuid(&item_id, "item_id")?;
        self.session_store
            .mark_inbox_item_opened(id)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 归档 / 取消归档收件箱条目。条目不存在时静默成功。
    pub async fn archive_inbox_item(&self, item_id: String, archived: bool) -> Result<(), JsValue> {
        let id = parse_uuid(&item_id, "item_id")?;
        self.session_store
            .archive_inbox_item(id, archived)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 软删除收件箱条目；`delete_local_files` 为真时连 OPFS 里的文件一起删。
    ///
    /// 与桌面 `delete_inbox_item(item_id, delete_local_files)` 同签名同语义：**是否连文件
    /// 一起删由宿主决定，端口只管账本**（`delete_inbox_item_record` 永远只软删记录）。
    ///
    /// 不删文件时那份 OPFS 副本会成为孤儿——记录一软删，`list`/`search`/`detail` 就都看不到
    /// 它了，配额却还占着，用户唯一的出路是浏览器的「清除站点数据」。所以这个入口不是锦上添花：
    /// 没有它，Web 端的每一次删除都在泄漏。
    ///
    /// 编排（顺序、失败处理、幂等）是**三端共用的领域规则**，住在
    /// [`swarmdrop_transfer::inbox::delete_inbox_item`]——本方法只做参数解析与错误转换。
    /// 「OPFS 的键要剥掉 `opfs:/` 前缀」那一层在 [`OpfsFileAccess::delete_finalized_file`]，
    /// 编排不需要知道哪一端用哪个字段。
    pub async fn delete_inbox_item(
        &self,
        item_id: String,
        delete_local_files: bool,
    ) -> Result<(), JsValue> {
        let id = parse_uuid(&item_id, "item_id")?;
        swarmdrop_transfer::inbox::delete_inbox_item(
            self.session_store.as_ref(),
            self.file_access.as_ref(),
            id,
            delete_local_files,
        )
        .await
        .map_err(WebError::from)?;
        Ok(())
    }

    /// 完成接收后，把 OPFS 里的文件读回成 blob URL 供 `<a download>` 下载。
    pub async fn download_url(&self, relative_path: String) -> Result<String, JsValue> {
        crate::opfs::export_blob_url(&relative_path)
            .await
            .map_err(|e| WebError::from(e).into())
    }

    /// 打开 OPFS 里的一个文件，返回 `File` 句柄（**不读字节**）。
    ///
    /// 缩略图管线的取图入口：`createImageBitmap` 只吃 `Blob`，所以这里给的是 `File` 本身
    /// 而不是 [`download_url`](Self::download_url) 那样的 blob URL——后者还得 `fetch` 一次
    /// 绕回 Blob，多一次拷贝，中间那个 URL 也必须记得 revoke。
    ///
    /// 非 secure origin 下 OPFS 整个不可用，这里会明确报错（而不是永久 pending），
    /// 前端据此降级到类型图标。
    pub async fn open_file(&self, relative_path: String) -> Result<web_sys::File, JsValue> {
        crate::opfs::open_file(&relative_path)
            .await
            .map_err(|e| WebError::from(e).into())
    }

    /// 传输事件流（逐条产出 `WebTransferEvent` 序列化对象）。**只能取一次**（单点消费）。
    pub fn events(&self) -> Result<TransferEventStream, JsValue> {
        let rx = self
            .events_rx
            .borrow_mut()
            .take()
            .ok_or_else(|| WebError::invalid_input("events() 只能取一次"))?;
        let stream = rx.map(|ev| Ok::<JsValue, JsValue>(crate::events::serialize_event(ev)));
        Ok(
            JsValue::from(wasm_streams::ReadableStream::from_stream(stream).into_raw())
                .unchecked_into(),
        )
    }

    /// 关停节点：NetManager::shutdown 取消内部 token（停 presence / infra / event-loop +
    /// transfer cleanup，drop Router 停路由）并关 Endpoint（drop Swarm → 断连）——
    /// 与 `WebNode.endpoint` 是同一 handle，无需再显式关一次。
    pub async fn close(self) {
        self.net_manager.shutdown().await;
    }
}

/// 浏览器环境的 [`OsInfo`]：UA 粗判 os、platform 固定 `"web"`、hostname 用浏览器名
/// （UI 按 `name || hostname` 回退显示，浏览器名比占位 "Device" 有辨识度）。
/// wasm 下 `OsInfo::default()` 的 env 探测恒 "unknown"，必须自建。
///
/// `pub(crate)` 是为了让 [`default_device_name`](crate::device_config::default_device_name)
/// 复用同一份 UA 判定：设置页 placeholder 与对端看到的默认名必须同源，否则会漂成
/// 「placeholder 写 Safari、对端看到 Browser」。
pub(crate) fn web_os_info() -> swarmdrop_host::device::OsInfo {
    let ua = crate::env::user_agent();
    let os = if ua.contains("Windows") {
        "windows"
    } else if ua.contains("Android") {
        "android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "ios"
    } else if ua.contains("Mac OS") {
        "macos"
    } else if ua.contains("Linux") {
        "linux"
    } else {
        "unknown"
    };
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Browser"
    };
    swarmdrop_host::device::OsInfo {
        name: None,
        hostname: browser.to_string(),
        os: os.to_string(),
        platform: "web".to_string(),
        arch: "wasm32".to_string(),
        capabilities: Vec::new(),
    }
}

/// [`InviteParseError`] → 给人看的一句话。
///
/// 分四种而不是拍成一句「邀请无效」：只有第一种是用户自己能修的（复制漏了、根本不是这类
/// 链接），后三种要说的都是「别再试了，让对方重发一条」。内层的技术细节（base32 偏移、
/// postcard 报错）对用户零信息量，只进 `tracing::debug!`。
fn invite_parse_message(error: &InviteParseError) -> String {
    match error {
        InviteParseError::Kind => "这不是一条配对邀请链接（检查是否复制完整）".to_owned(),
        InviteParseError::Encoding(_) => "邀请链接已损坏或被截断（试试重新复制一次）".to_owned(),
        InviteParseError::Postcard(_) => "邀请格式无法解析（可能由更新版本生成）".to_owned(),
        // Verify 的内层本就是给人看的一句话（签名无效 / 地址提示非法 …），原样透出。
        InviteParseError::Verify(reason) => (*reason).to_owned(),
    }
}

fn parse_session_id(s: &str) -> Result<Uuid, WebError> {
    Uuid::parse_str(s.trim()).map_err(|e| WebError::invalid_input(format!("非法 session_id: {e}")))
}

/// 解析 UUID 入参（收件箱条目 id 等），错误里带上字段名。
fn parse_uuid(s: &str, what: &str) -> Result<Uuid, JsValue> {
    Uuid::parse_str(s.trim())
        .map_err(|e| WebError::invalid_input(format!("非法 {what}: {e}")).into())
}

/// 解析 base58 身份串为 [`NodeId`]（`infra_drop` / `infra_until_active` 入参）。
fn parse_node_id(s: &str) -> Result<NodeId, JsValue> {
    s.trim()
        .parse::<NodeId>()
        .map_err(|e| WebError::invalid_input(format!("非法 NodeId: {e}")).into())
}

/// 解析带 `/p2p/<id>` 的 multiaddr 为 `(目标 NodeId, 完整 Addr)`。
///
/// 用 `Addr::p2p_node_id()`（net-base）取**末位** P2p 段——circuit 地址
/// `/…/p2p/RELAY/p2p-circuit/p2p/TARGET` 的目标身份在末位，天真的字符串切分会抓错。
/// 地址整体（含 `/p2p/` 段）交给 dial，libp2p 会据此校验对端身份。
fn split_p2p_addr(s: &str) -> Result<(NodeId, swarmdrop_net::Addr), JsValue> {
    let addr = s
        .trim()
        .parse::<swarmdrop_net::Addr>()
        .map_err(|e| WebError::invalid_input(format!("地址解析失败: {e}")))?;
    let id = addr
        .p2p_node_id()
        .ok_or_else(|| WebError::invalid_input("地址须含 /p2p/<node-id>"))?;
    Ok((id, addr))
}
