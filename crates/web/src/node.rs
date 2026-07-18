//! `WebNode`：Web 壳的组合根 + wasm-bindgen API 面。
//!
//! 端口全用 Web 实现装配 `TransferManager`（内存 store / OPFS / ReadableStream 事件），
//! Router 只挂 transfer 控制面 + 数据面（无 pairing——[`WebPeerDirectory`] 对任意对端返回
//! 「需手动确认」的合成设备，让入站 offer 走缓存 + 手动 accept 路径）。

use std::cell::RefCell;
use std::sync::Arc;

use futures::StreamExt;
use serde::Serialize;
use swarmdrop_host::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_host::{CoreSaveLocation, FileAccess, FileSourceId};
use swarmdrop_net::{DhtConfig, Endpoint, NodeAddr, NodeId, RelayState, Router, presets};
use swarmdrop_transfer::HostEnumeratedFile;
use swarmdrop_transfer::events::TransferEventSink;
use swarmdrop_transfer::incoming::TransferCtrlService;
use swarmdrop_transfer::manager::TransferManager;
use swarmdrop_transfer::peer::PeerDirectory;
use swarmdrop_transfer::protocol::{
    FileInfo, TRANSFER_CTRL, TRANSFER_CTRL_PROTOCOL, TRANSFER_DATA_PROTOCOL, TransferOrigin,
};
use swarmdrop_transfer::store::TransferStore;
use swarmdrop_transfer::wire::TransferDataHandler;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::File;

use crate::error::{WebError, js_err};
use crate::events::WebEventSink;
use crate::file_access::OpfsFileAccess;
use crate::store::MemorySessionStore;
use crate::{identity, share_code};

/// 浏览器传输端节点。
#[wasm_bindgen]
pub struct WebNode {
    endpoint: Endpoint,
    manager: Arc<TransferManager>,
    _router: Router,
    file_access: Arc<OpfsFileAccess>,
    events_rx: RefCell<
        Option<
            futures::channel::mpsc::UnboundedReceiver<swarmdrop_transfer::events::TransferEvent>,
        >,
    >,
    cancel: CancellationToken,
}

#[wasm_bindgen]
impl WebNode {
    /// 建节点：持久化身份（Window=localStorage / Worker=OPFS）→ Browser preset + DHT client
    /// → 装配 TransferManager + Router（transfer-ctrl / transfer-data）。Window 与 Worker
    /// 环境通吃（Worker 里勿拨 webrtc-direct 地址——webrtc-websys dial 碰 window 会 panic）。
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
        let endpoint = Endpoint::builder()
            .secret_key(secret)
            .preset(presets::Browser)
            .identify_protocol("/swarmdrop/2.0.0")
            .agent_version("swarmdrop-web/0.1")
            .dht(DhtConfig::default())
            .bind()
            .await
            .map_err(js_err)?;

        let store: Arc<dyn TransferStore> = Arc::new(MemorySessionStore::new());
        let file_access_impl = Arc::new(OpfsFileAccess::new());
        let file_access: Arc<dyn FileAccess> = file_access_impl.clone();
        let (sink, events_rx) = WebEventSink::new();
        let events: Arc<dyn TransferEventSink> = Arc::new(sink);
        let manager = Arc::new(TransferManager::new(
            endpoint.clone(),
            events,
            store,
            file_access,
        ));

        let cancel = CancellationToken::new();
        manager.spawn_cleanup_task(cancel.clone());

        // PeerDirectory 对任意对端返回「需手动确认」的合成设备（Collaborator，auto_accept=false）：
        // incoming.rs 对未配对（None）offer 硬拒 NotPaired（桌面安全边界），故 Web 无配对时必须
        // 给个 Some——不改 transfer，语义正是「陌生设备手动确认」。
        let pairing: Arc<dyn PeerDirectory> = Arc::new(WebPeerDirectory);
        let router = Router::builder(endpoint.clone())
            .accept(
                TRANSFER_CTRL_PROTOCOL,
                TRANSFER_CTRL.handler(TransferCtrlService::new(
                    manager.clone(),
                    pairing,
                    endpoint.clone(),
                    None,
                )),
            )
            .accept(
                TRANSFER_DATA_PROTOCOL,
                TransferDataHandler::new(manager.clone()),
            )
            .spawn();

        Ok(WebNode {
            endpoint,
            manager,
            _router: router,
            file_access: file_access_impl,
            events_rx: RefCell::new(Some(events_rx)),
            cancel,
        })
    }

    /// 本节点身份（base58）。
    pub fn node_id(&self) -> String {
        self.endpoint.node_id().to_string()
    }

    /// 拨任意 multiaddr（`.../ws` 或 `.../webrtc-direct/certhash/...`，须带 `/p2p/<id>`）。
    pub async fn connect(&self, addr: String) -> Result<String, JsValue> {
        let (id, addr) = split_p2p_addr(&addr)?;
        let info = self
            .endpoint
            .connect(NodeAddr::with_addrs(id, vec![addr]))
            .await
            .map_err(js_err)?;
        Ok(format!(
            "connected: path={:?} addr={}",
            info.path, info.addr
        ))
    }

    /// 经 helper 请求 circuit reservation（浏览器被动接收连接的唯一入口），返回 circuit 地址。
    pub async fn reserve(&self, helper_addr: String) -> Result<String, JsValue> {
        let (id, addr) = split_p2p_addr(&helper_addr)?;
        self.endpoint
            .ensure_relay_reservation(NodeAddr::with_addrs(id, vec![addr]))
            .await
            .map_err(js_err)?;
        let mut relays = self.endpoint.watch_relays();
        loop {
            if relays
                .get()
                .get(&id)
                .is_some_and(|s| *s == RelayState::Active)
            {
                return Ok(format!(
                    "{helper_addr}/p2p-circuit/p2p/{}",
                    self.endpoint.node_id()
                ));
            }
            if relays.updated().await.is_none() {
                return Err(WebError::network("endpoint 已关闭").into());
            }
        }
    }

    /// 查分享码 → 返回对端 `NodeAddr` 的 JSON（`{ id, addrs }`），并注册进地址簿。
    /// 前置：本节点已 connect 到一个 DHT-capable helper（浏览器不可达 TCP bootstrap）。
    pub async fn lookup_share_code(&self, code: String) -> Result<String, JsValue> {
        let node_addr = share_code::lookup(&self.endpoint, &code).await?;
        serde_json::to_string(&node_addr)
            .map_err(|e| WebError::network(format!("序列化 NodeAddr 失败: {e}")).into())
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

    /// 当前挂起（待确认）的入站 offer 列表（JSON 数组）。
    pub fn pending_offers(&self) -> Result<JsValue, JsValue> {
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
        serde_wasm_bindgen::to_value(&offers)
            .map_err(|e| WebError::network(format!("序列化 offers 失败: {e}")).into())
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
        crate::file_access::export_blob_url(&relative_path)
            .await
            .map_err(|e| WebError::from(e).into())
    }

    /// 传输事件流（`TransferEvent` 序列化后的对象）。**只能取一次**（单点消费）。
    pub fn events(&self) -> Result<web_sys::ReadableStream, JsValue> {
        let rx = self
            .events_rx
            .borrow_mut()
            .take()
            .ok_or_else(|| WebError::invalid_input("events() 只能取一次"))?;
        let stream = rx.map(|ev| Ok::<JsValue, JsValue>(crate::events::serialize_event(ev)));
        Ok(wasm_streams::ReadableStream::from_stream(stream).into_raw())
    }

    /// 关停节点（取消后台任务 + 关 Endpoint，drop Router 停路由）。
    pub async fn close(self) {
        self.cancel.cancel();
        self.endpoint.close().await;
    }
}

/// 无配对目录：对任意对端返回「陌生、需手动确认」的合成设备（见 spawn 注释）。
struct WebPeerDirectory;

impl PeerDirectory for WebPeerDirectory {
    fn get_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo> {
        // Collaborator + trust_confirmed=true 但 auto_accept=false → policy RequireConfirmation。
        Some(PairedDeviceInfo::new(
            *peer_id,
            OsInfo::unknown_from_peer_id(peer_id),
            js_sys::Date::now() as i64,
        ))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OfferJson {
    session_id: String,
    peer_id: String,
    peer_name: String,
    total_size: u64,
    files: Vec<FileInfo>,
}

fn parse_session_id(s: &str) -> Result<Uuid, WebError> {
    Uuid::parse_str(s.trim()).map_err(|e| WebError::invalid_input(format!("非法 session_id: {e}")))
}

/// 把 `<addr>/p2p/<id>` 拆成 `(NodeId, Addr)`。
fn split_p2p_addr(s: &str) -> Result<(NodeId, swarmdrop_net::Addr), JsValue> {
    let s = s.trim();
    let (addr, id) = s
        .rsplit_once("/p2p/")
        .ok_or_else(|| WebError::invalid_input("地址须以 /p2p/<node-id> 结尾"))?;
    let id = id
        .parse::<NodeId>()
        .map_err(|e| WebError::invalid_input(format!("NodeId 解析失败: {e}")))?;
    let addr = addr
        .parse::<swarmdrop_net::Addr>()
        .map_err(|e| WebError::invalid_input(format!("地址解析失败: {e}")))?;
    Ok((id, addr))
}
