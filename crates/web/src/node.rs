//! `WebNode`：Web 壳的 wasm-bindgen API 面。
//!
//! 浏览器节点**包一层 core 的组合根** [`start_node`]（与桌面/移动同源装配），注入 Browser
//! [`EndpointProfile`] + Web 端口实现（内存 store / OPFS / WebEventSink transfer 事件流）。走
//! 完整 [`NetManager`] + 3 协议 Router（含 pairing）：invite 配对经 `pair_with_invite`，配对记录
//! 内存态（IndexedDB 持久化属后续）。NetManager 侧 pairing/device 事件走最小
//! [`WebEventBus`](crate::event_bus)（consume-invite 路径的确认在邀请方桌面，浏览器侧暂不 surface）。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use swarmdrop_core::host::EventBus;
use swarmdrop_core::network::event_loop::spawn_event_loop;
use swarmdrop_core::network::{DiscoveryMode, NetManager, NetworkRuntimeConfig};
use swarmdrop_core::protocol::pairing::{PairingRefuseReason, PairingResponse};
use swarmdrop_core::runtime::{EndpointProfile, start_node};
use swarmdrop_host::device::OsInfo;
use swarmdrop_host::{CoreSaveLocation, FileAccess, FileSourceId};
use swarmdrop_invite::TransportPolicy;
use swarmdrop_net::{Endpoint, NodeAddr, NodeId, RelayState, SecretKey};
use swarmdrop_transfer::HostEnumeratedFile;
use swarmdrop_transfer::events::TransferEventSink;
use swarmdrop_transfer::manager::TransferManager;
use swarmdrop_transfer::protocol::TransferOrigin;
use swarmdrop_transfer::store::TransferStore;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::File;

use crate::error::{WebError, js_err};
use crate::event_bus::{PendingPairings, WebEventBus};
use crate::events::WebEventSink;
use crate::file_access::OpfsFileAccess;
use crate::identity;
use crate::store::MemorySessionStore;
use crate::types::{ConnectionJson, OfferJson, PendingPairingJson, RelayInfoJson, RelayStateKind};

// specta 导出的 TS 类型（static/types/bindings.ts，由 `cargo test -p swarmdrop-web
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
    /// `events()` 的返回：逐条产出 [`WebTransferEvent`] 序列化对象的流。
    #[wasm_bindgen(typescript_type = "ReadableStream<WebTransferEvent>")]
    pub type TransferEventStream;
    /// `connect()` 的返回。
    #[wasm_bindgen(typescript_type = "ConnectionJson")]
    pub type ConnectionJsonJs;
    /// `relays_state()` 的返回：`RelayInfoJson[]`。
    #[wasm_bindgen(typescript_type = "RelayInfoJson[]")]
    pub type RelayInfoArray;
    /// `relays_changed()` 的返回：每次 relay 状态变化产出一份全量快照。
    #[wasm_bindgen(typescript_type = "ReadableStream<RelayInfoJson[]>")]
    pub type RelayChangedStream;
}

/// `until_active` 的契约级默认超时：即便调用方不传 signal，Promise 也在
/// 有限时间内 settle（「任何 JS Promise 有限时间 settle」不变量的本地兜底）。
/// 30s 是对外承诺的 API 默认值，不与下层内部常量（supervisor tick 间隔 /
/// connect 超时）联动——调用方要更短的耐心用 `AbortSignal.timeout()` 表达。
const UNTIL_ACTIVE_CAP: Duration = Duration::from_secs(30);

/// `watch_relays` 快照 → JS 投影（`RelayInfoJson[]`）。
fn relay_info_json(map: &BTreeMap<NodeId, RelayState>) -> Vec<RelayInfoJson> {
    map.iter()
        .map(|(id, state)| {
            let (kind, circuit_addr, attempts, last_error) = match state {
                RelayState::Connecting { attempt } => {
                    (RelayStateKind::Connecting, None, *attempt, None)
                }
                RelayState::Active { circuit_addr } => (
                    RelayStateKind::Active,
                    Some(circuit_addr.to_string()),
                    0,
                    None,
                ),
                RelayState::Failed {
                    attempts,
                    last_error,
                } => (
                    RelayStateKind::Failed,
                    None,
                    *attempts,
                    Some(last_error.clone()),
                ),
            };
            RelayInfoJson {
                id: id.to_string(),
                state: kind,
                circuit_addr,
                attempts,
                last_error,
            }
        })
        .collect()
}

/// serde 可序列化值 → 具名 TS 类型的 JsValue（`unchecked_into` 到 typescript_type 包装）。
fn to_js_typed<T: serde::Serialize, R: JsCast>(value: &T, what: &str) -> Result<R, JsValue> {
    serde_wasm_bindgen::to_value(value)
        .map(JsValue::unchecked_into)
        .map_err(|e| WebError::network(format!("序列化{what}失败: {e}")).into())
}

/// 浏览器传输端节点。
#[wasm_bindgen]
pub struct WebNode {
    endpoint: Endpoint,
    /// transfer 控制面（send / offers / accept / resume）——从 [`NetManager`] 取出的 Arc。
    manager: Arc<TransferManager>,
    /// core 网络管理器：pairing（invite 配对）+ devices + shutdown（含 cleanup task 生命周期）。
    net_manager: NetManager<TransferManager>,
    /// 本机私钥（`generate_invite` 签名邀请用；start_node 吃的是它的 clone）。
    secret: SecretKey,
    /// 本机 OsInfo（`generate_invite` 的 display_hint 用）。
    os_info: OsInfo,
    /// 入站配对请求队列（browser-as-inviter：桌面消费本机 invite 后本机弹确认）。
    pending_pairings: PendingPairings,
    file_access: Arc<OpfsFileAccess>,
    events_rx: RefCell<
        Option<
            futures::channel::mpsc::UnboundedReceiver<swarmdrop_transfer::events::TransferEvent>,
        >,
    >,
}

#[wasm_bindgen]
impl WebNode {
    /// 建节点：持久化身份（Window=localStorage / Worker=OPFS）→ 包 core 组合根 [`start_node`]
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
        let os_info = web_os_info();

        // Web 端口：内存 store / OPFS 落盘 / WebEventSink（transfer 事件直连 events() 流）。
        let file_access_impl = Arc::new(OpfsFileAccess::new());
        let file_access: Arc<dyn FileAccess> = file_access_impl.clone();
        let (sink, events_rx) = WebEventSink::new();
        let transfer_events: Arc<dyn TransferEventSink> = Arc::new(sink);
        // NetManager 侧事件的 bus：捕获入站配对请求（browser-as-inviter），其余记日志。
        // transfer 事件不经此（走 WebEventSink → events() 流）。
        let (event_bus_impl, pending_pairings) = WebEventBus::new();
        let event_bus: Arc<dyn EventBus> = Arc::new(event_bus_impl);

        let file_access_for_factory = file_access.clone();
        let started = start_node(
            secret.clone(),
            None,
            os_info.clone(),
            Vec::new(), // 已配对设备：内存态起步（IndexedDB 持久化属后续工程）
            // LanOnly：浏览器拨不了 TCP/QUIC 内置 bootstrap，跳过它免得 infra 反复空拨刷屏；
            // LAN 配对经直连 ws + invite，不需 DHT bootstrap（公网可达待 webrtc-direct bootstrap）。
            NetworkRuntimeConfig {
                discovery_mode: DiscoveryMode::LanOnly,
                ..NetworkRuntimeConfig::default()
            },
            EndpointProfile::Browser,
            event_bus.clone(),
            None, // 浏览器无系统通知
            move |endpoint| {
                TransferManager::new(
                    endpoint,
                    transfer_events,
                    Arc::new(MemorySessionStore::new()) as Arc<dyn TransferStore>,
                    file_access_for_factory,
                )
            },
        )
        .await
        .map_err(WebError::from)?;

        let endpoint = started.endpoint.clone();
        let net_manager = started.manager;
        let manager = net_manager.transfer_arc();
        // 事件循环（presence / infra / 状态刷新）随 router 同生命周期，n0-future spawn（wasm 友好）。
        spawn_event_loop(
            started.events,
            net_manager.shared_refs(),
            event_bus,
            started.router,
        );

        Ok(WebNode {
            endpoint,
            manager,
            net_manager,
            secret,
            os_info,
            pending_pairings,
            file_access: file_access_impl,
            events_rx: RefCell::new(Some(events_rx)),
        })
    }

    /// 本节点身份（base58）。
    pub fn node_id(&self) -> String {
        self.endpoint.node_id().to_string()
    }

    /// 拨任意 multiaddr（`.../ws` 或 `.../webrtc-direct/certhash/...`，须带 `/p2p/<id>`）。
    /// 返回结构化的连接信息（`{ path: "local"|"direct"|"relayed", addr }`）。
    ///
    /// `signal`（可选）：标准 `AbortSignal`——超时组合用平台原语表达
    /// （`AbortSignal.timeout(5000)` / `AbortSignal.any([...])`）。abort 时 Promise
    /// 立即以 `{ kind: "aborted" }` reject；**abort ≠ 撤回拨号**（在途拨号继续到
    /// 自然失败，无常驻意图残留）。不传 signal 时由内核兜底超时（Browser 15s）
    /// 保证有限时间内 settle。
    pub async fn connect(
        &self,
        addr: String,
        signal: Option<web_sys::AbortSignal>,
    ) -> Result<ConnectionJsonJs, JsValue> {
        let (id, addr) = split_p2p_addr(&addr)?;
        let connect = self.endpoint.connect(NodeAddr::with_addrs(id, vec![addr]));
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

    /// 受邀方：消费邀请串完成**真配对握手**。
    ///
    /// `pair_with_invite` 解码验签 → TTL 预检 → 按 `TransportPolicy` 过滤地址 → 连邀请方出示
    /// capability（`PairingMethod::Invite`）→ 邀请方（桌面）校验 CAS 一次性消费 + 用户确认 →
    /// 双方写配对记录。身份 pin 由握手强制（连到的必然是 `inviter_id`）。成功返回已配对对端的
    /// NodeId（base58）；确认发生在**邀请方**侧，浏览器侧无需交互。配对后该对端进入本机信任
    /// 表，双向传输（收 / 发）不再被 `NotPaired` 拦。
    pub async fn connect_invite(&self, invite: String) -> Result<String, JsValue> {
        let (response, paired) = self
            .net_manager
            .pairing()
            .pair_with_invite(&invite)
            .await
            .map_err(WebError::from)?;
        match response {
            PairingResponse::Success => {
                Ok(paired.map(|d| d.peer_id.to_string()).unwrap_or_default())
            }
            _ => Err(WebError::network("邀请方拒绝了配对或配对未成功").into()),
        }
    }

    /// 发起方（browser-as-inviter）：生成一次性签名邀请串，供桌面/移动扫码或粘贴消费。
    ///
    /// `local_only=true` 走 LocalOnly（受邀方只用私网地址）。邀请自包含本机 dialable 地址提示——
    /// 浏览器不 listen 本地 socket，其可达地址来自 **relay reservation**（circuit 地址）；故桌面要
    /// 拨得到本机，本机需先经 [`relays_ensure`](Self::relays_ensure) 在某 helper 上建 reservation
    /// （等到 `active`），否则邀请里无可拨地址、消费方连不上。
    pub fn generate_invite(&self, local_only: bool) -> Result<String, JsValue> {
        let policy = if local_only {
            TransportPolicy::LocalOnly
        } else {
            TransportPolicy::Auto
        };
        Ok(self
            .net_manager
            .pairing()
            .encode_invite(&self.secret, policy, &self.os_info))
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
    ) -> Result<(), JsValue> {
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
        self.net_manager
            .pairing()
            .respond_pairing_request(id, response)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    // ── relay 意图（声明式集合，替代一次性 RPC 形态的 reserve()）──
    //
    // reservation 是「持续维持的可达状态」而非「一次完成的动作」：命令（ensure/
    // drop）改期望状态、同步返回；实际状态经查询（relays_state）与订阅
    // （relays_changed）到达。意图生命周期与单次等待的耐心（AbortSignal）解耦。

    /// 登记一个 relay helper 的常驻可达意图（幂等，同步返回）。
    ///
    /// 浏览器被动接收连接的唯一入口。拨号 / reservation / 断线重建由 core 的
    /// InfraSupervisor 统一收敛（最迟 1s 内启动第一轮，失败退避重试）；进度经
    /// [`relays_state`](Self::relays_state) / [`relays_changed`](Self::relays_changed)
    /// 观测，或用 [`relays_until_active`](Self::relays_until_active) 等首次建立。
    ///
    /// 返回 helper 的 base58 NodeId——即 `relays_drop` / `relays_until_active` 的
    /// 入参，调用方直接串联，无需自行解析 multiaddr 的 `/p2p/` 段。
    pub fn relays_ensure(&self, helper_addr: String) -> Result<String, JsValue> {
        let (id, addr) = split_p2p_addr(&helper_addr)?;
        self.net_manager
            .ensure_relay_intent(NodeAddr::with_addrs(id, vec![addr]));
        Ok(id.to_string())
    }

    /// 撤销 relay 意图（[`relays_ensure`](Self::relays_ensure) 的对称面）。
    ///
    /// **真撤销**而非停止等待：停止后台收敛重试、关闭 circuit listener、立刻
    /// 断开与该 helper 的连接（含中止在途拨号），条目从状态集合消失。
    pub async fn relays_drop(&self, helper_id: String) -> Result<(), JsValue> {
        let id = parse_node_id(&helper_id)?;
        self.net_manager
            .remove_relay_intent(id)
            .await
            .map_err(WebError::from)?;
        Ok(())
    }

    /// 全量 relay 状态快照（`{ id, state, circuitAddr?, attempts, lastError? }[]`）。
    pub fn relays_state(&self) -> Result<RelayInfoArray, JsValue> {
        to_js_typed(
            &relay_info_json(&self.endpoint.watch_relays().get()),
            "relay 状态",
        )
    }

    /// relay 状态变化流：每次变化产出一份全量快照（可直接 setState）。
    /// 可多次调用（每次独立订阅），与 `events()` 的单点消费不同。
    pub fn relays_changed(&self) -> RelayChangedStream {
        let watcher = self.endpoint.watch_relays();
        let stream = futures::stream::unfold(watcher, |mut w| async move {
            let map = w.updated().await?;
            let value =
                serde_wasm_bindgen::to_value(&relay_info_json(&map)).unwrap_or(JsValue::NULL);
            Some((Ok::<JsValue, JsValue>(value), w))
        });
        JsValue::from(wasm_streams::ReadableStream::from_stream(stream).into_raw()).unchecked_into()
    }

    /// 等待某 relay 首次进入 `active`，resolve 出 circuit 可达地址（内核拼装）。
    ///
    /// 观察到 `failed` 时**立即 reject**（把「要不要再等下一轮退避」还给调用方），
    /// 意图保留——要停止后台收敛请调 [`relays_drop`](Self::relays_drop)。
    /// `signal`（可选）：abort 只是不再等待，同样不改变意图生命周期。
    /// 不传 signal 时 30s 兜底超时保证 Promise 有限时间内 settle。
    pub async fn relays_until_active(
        &self,
        helper_id: String,
        signal: Option<web_sys::AbortSignal>,
    ) -> Result<String, JsValue> {
        let id = parse_node_id(&helper_id)?;
        let mut relays = self.endpoint.watch_relays();
        let wait = async move {
            // 首轮读快照，此后消费 updated() 的返回值——每次唤醒只做一份 map 拷贝
            let mut map = relays.get();
            loop {
                match map.get(&id) {
                    Some(RelayState::Active { circuit_addr }) => {
                        return Ok(circuit_addr.to_string());
                    }
                    Some(RelayState::Failed {
                        attempts,
                        last_error,
                    }) => {
                        return Err(WebError::network(format!(
                            "relay 建立失败（第 {attempts} 轮）: {last_error}"
                        )));
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
        let mut entries = Vec::with_capacity(files.len());
        for file in &files {
            let name = file.name();
            let source_id = FileSourceId(name.clone());
            self.file_access
                .register_source(source_id.clone(), file.clone());
            entries.push(HostEnumeratedFile {
                source_id,
                name: name.clone(),
                relative_path: name,
                size: file.size() as u64,
            });
        }

        let prepared_id = Uuid::new_v4();
        let prepared = self
            .manager
            .prepare(prepared_id, entries)
            .await
            .map_err(WebError::from)?;
        let file_ids: Vec<u32> = prepared.files.iter().map(|f| f.file_id).collect();

        let result = self
            .manager
            .send_offer(&prepared_id, &to, "web", &file_ids, TransferOrigin::Human)
            .await
            .map_err(WebError::from)?;
        Ok(result.session_id.to_string())
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

    /// 手动发起断点续传（对某 suspended 会话）。
    pub async fn resume(&self, session_id: String) -> Result<(), JsValue> {
        let sid = parse_session_id(&session_id)?;
        self.manager
            .initiate_resume(sid)
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
fn web_os_info() -> swarmdrop_host::device::OsInfo {
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

fn parse_session_id(s: &str) -> Result<Uuid, WebError> {
    Uuid::parse_str(s.trim()).map_err(|e| WebError::invalid_input(format!("非法 session_id: {e}")))
}

/// 解析 base58 身份串为 [`NodeId`]（`relays_drop` / `relays_until_active` 入参）。
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
