use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dashmap::DashMap;
use swarmdrop_net::{AcceptError, Addr, CallOptions, Dht, DhtKey, Endpoint, NodeId, RpcService};
use tokio::sync::oneshot;

use super::code::{PairingCodeInfo, ShareCodeRecord};
use crate::device::{OsInfo, PairedDeviceInfo};
use crate::device_manager::DeviceManager;
use crate::host::{CoreEvent, EventBus, Notification, Notifier};
use crate::protocol::{
    PAIRING, PairingMethod, PairingRefuseReason, PairingRequest, PairingResponse,
};
use crate::{AppError, AppResult};

/// 分享码 DHT 命名空间（迁自旧栈 `dht_key::NS_SHARE_CODE`）。
const SHARE_CODE_NS: &str = "/swarmdrop/share-code/";

/// 出站配对调用超时（对齐旧栈 req_resp_timeout，容纳对端等用户决策的长交互）。
const PAIRING_CALL_TIMEOUT: Duration = Duration::from_secs(180);

/// 入站配对请求待决表最长等待（超时回收，避免 handler 任务无限挂起）。
const PENDING_INBOUND_TIMEOUT: Duration = Duration::from_secs(180);

/// 入站配对请求的待决上下文。
///
/// 新内核 RPC handler 天然长 await：handler 存 `responder` 后 await 用户决策，
/// UI 命令 `respond_pairing_request` 解决它后 handler 返回 Response。
struct PendingInbound {
    peer_id: NodeId,
    os_info: OsInfo,
    method: PairingMethod,
    responder: oneshot::Sender<PairingResponse>,
}

/// 配对管理器（兼配对 typed RPC 服务）。
///
/// 管理配对码生成/查询、出站配对请求、入站请求的用户决策编排，以及已配对设备的
/// 增删查。在线宣告与已配对设备的 presence 维持见 [`crate::presence`]。
pub struct PairingManager {
    endpoint: Endpoint,
    /// 当前活跃的配对码（单例，同一时刻最多一个）
    active_code: Mutex<Option<PairingCodeInfo>>,
    /// 已配对设备（与 DeviceManager 共享读取）
    paired_devices: Arc<DashMap<NodeId, PairedDeviceInfo>>,
    /// 入站请求待决表（correlation id → 上下文 + oneshot sender）
    pending_inbound: DashMap<u64, PendingInbound>,
    /// correlation id 分配器（进程内自增；不再是旧内核 pending 响应 id）
    next_pending_id: AtomicU64,
    /// get_device_info 查询时缓存对端 OsInfo，request_pairing 成功后使用
    discovered_peers: DashMap<NodeId, OsInfo>,
    /// Direct 配对的局域网校验依据（`is_lan_discovered`）
    devices: Arc<DeviceManager>,
    /// 入站请求到达时发 [`CoreEvent::PairingRequestReceived`]
    event_bus: Arc<dyn EventBus>,
    /// 入站请求到达时的系统通知（桌面端；移动端传 None）
    notifier: Option<Arc<dyn Notifier>>,
}

impl PairingManager {
    pub fn new(
        endpoint: Endpoint,
        paired_devices: Arc<DashMap<NodeId, PairedDeviceInfo>>,
        devices: Arc<DeviceManager>,
        event_bus: Arc<dyn EventBus>,
        notifier: Option<Arc<dyn Notifier>>,
    ) -> Self {
        Self {
            endpoint,
            active_code: Mutex::new(None),
            paired_devices,
            pending_inbound: DashMap::new(),
            next_pending_id: AtomicU64::new(0),
            discovered_peers: DashMap::new(),
            devices,
            event_bus,
            notifier,
        }
    }

    fn dht(&self) -> AppResult<&Dht> {
        self.endpoint
            .dht()
            .ok_or_else(|| AppError::Network("DHT 未启用".into()))
    }

    fn share_code_key(code: &str) -> DhtKey {
        DhtKey::namespaced(SHARE_CODE_NS, code.as_bytes())
    }

    /// 本机可供对端拨号的地址（监听 ∪ 外部确认地址，去重）。
    fn shareable_addrs(&self) -> Vec<Addr> {
        self.endpoint.watch_addrs().get().dialable()
    }

    // === 配对码管理 ===

    pub async fn generate_code(&self, expires_in_secs: u64) -> AppResult<PairingCodeInfo> {
        let code_info = PairingCodeInfo::generate(expires_in_secs);

        // 嵌入本机可达地址，供对方 dial 时使用
        let mut record_data = ShareCodeRecord::from(&code_info);
        record_data.listen_addrs = self.shareable_addrs();

        self.dht()?
            .put(
                Self::share_code_key(&code_info.code),
                serde_json::to_vec(&record_data)?,
                Some(Duration::from_secs(expires_in_secs)),
            )
            .await
            .map_err(|e| AppError::Network(format!("发布分享码失败: {e}")))?;

        // 覆盖旧码（旧 DHT 记录靠 TTL 自然过期，无需显式删除）
        *self.active_code.lock().unwrap() = Some(code_info.clone());

        Ok(code_info)
    }

    // === 配对流程 ===

    /// 查询配对码对应的设备信息，并缓存 OsInfo 供后续 request_pairing 使用
    pub async fn get_device_info(&self, code: &str) -> AppResult<(NodeId, ShareCodeRecord)> {
        let record = self
            .dht()?
            .get(Self::share_code_key(code))
            .await
            .map_err(|e| match e {
                swarmdrop_net::DhtError::NotFound => AppError::InvalidCode,
                other => AppError::Network(format!("查询分享码失败: {other}")),
            })?;

        let peer_id = record.publisher.ok_or(AppError::InvalidCode)?;
        let share_record = serde_json::from_slice::<ShareCodeRecord>(&record.value)?;

        // 分享码 record 自带过期时间（unix 秒）
        if share_record.expires_at < chrono::Utc::now().timestamp() {
            return Err(AppError::ExpiredCode);
        }

        // 将记录中的地址注册到地址簿，确保后续拨号能找到对方
        if !share_record.listen_addrs.is_empty() {
            self.endpoint
                .add_addrs(peer_id, share_record.listen_addrs.clone())
                .await
                .map_err(|e| AppError::Network(format!("注册对端地址失败: {e}")))?;
        }

        // 缓存对端 OsInfo，request_pairing 成功后用于构造 PairedDeviceInfo
        self.discovered_peers
            .insert(peer_id, share_record.os_info.clone());

        Ok((peer_id, share_record))
    }

    /// 发起配对请求
    ///
    /// 返回 `(PairingResponse, Option<PairedDeviceInfo>)`：
    /// - 对方接受 → 自动添加到已配对设备，返回 `Some(info)`
    /// - 对方拒绝 → 返回 `None`
    pub async fn request_pairing(
        &self,
        peer_id: NodeId,
        method: PairingMethod,
        addrs: Option<Vec<Addr>>,
    ) -> AppResult<(PairingResponse, Option<PairedDeviceInfo>)> {
        if let Some(addrs) = addrs.filter(|a| !a.is_empty()) {
            self.endpoint
                .add_addrs(peer_id, addrs)
                .await
                .map_err(|e| AppError::Network(format!("注册对端地址失败: {e}")))?;
        }

        let req = PairingRequest {
            os_info: OsInfo::default(),
            method,
            timestamp: chrono::Utc::now().timestamp(),
        };
        // RPC.call 内部按需拨号（复刻旧栈 dial + send_request）
        let res = PAIRING
            .call_with(
                &self.endpoint,
                peer_id,
                &req,
                CallOptions {
                    timeout: PAIRING_CALL_TIMEOUT,
                },
            )
            .await
            .map_err(|e| AppError::Network(format!("发送配对请求失败: {e}")))?;

        match res {
            PairingResponse::Success => {
                let os_info = self
                    .discovered_peers
                    .remove(&peer_id)
                    .map(|(_, info)| info)
                    .unwrap_or_else(|| OsInfo::unknown_from_peer_id(&peer_id));

                let info =
                    PairedDeviceInfo::new(peer_id, os_info, chrono::Utc::now().timestamp_millis());
                self.paired_devices.insert(peer_id, info.clone());

                Ok((PairingResponse::Success, Some(info)))
            }
            resp => Ok((resp, None)),
        }
    }

    // === 入站请求处理（RPC handler） ===

    /// 处理一个入站配对请求：发事件 + 通知，await 用户决策后返回 Response。
    ///
    /// Direct 无配对码凭证，唯一授权依据是「对端在本机 mDNS 多播域内」。这道校验必须
    /// 在发事件之前：否则任意远程 peer 都能靠一个 Direct 请求让本机弹窗 + 推系统通知。
    /// 非局域网的 Direct 请求不缓存、不弹窗、不通知、不回响应（断流），不向扫描者泄露在线。
    async fn handle_inbound(
        &self,
        from: NodeId,
        req: PairingRequest,
    ) -> Result<PairingResponse, AcceptError> {
        if matches!(req.method, PairingMethod::Direct) && !self.devices.is_lan_discovered(&from) {
            tracing::warn!("拒绝非局域网 peer 的 Direct 配对请求: {from}");
            return Err(AcceptError::from_err(AppError::Network(
                "direct pairing from non-LAN peer refused".into(),
            )));
        }

        let (tx, rx) = oneshot::channel();
        let pending_id = self.next_pending_id.fetch_add(1, Ordering::Relaxed);
        self.pending_inbound.insert(
            pending_id,
            PendingInbound {
                peer_id: from,
                os_info: req.os_info.clone(),
                method: req.method.clone(),
                responder: tx,
            },
        );

        let _ = self
            .event_bus
            .publish(CoreEvent::PairingRequestReceived {
                peer_id: from,
                pending_id,
                request: req.clone(),
            })
            .await;
        if let Some(notifier) = &self.notifier {
            let _ = notifier
                .notify_if_unfocused(Notification::PairingRequest {
                    hostname: req.os_info.hostname.clone(),
                })
                .await;
        }

        // await 用户决策；超时或 sender 被 drop（respond 校验失败 / 回收）→ 婉拒
        match tokio::time::timeout(PENDING_INBOUND_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            _ => {
                self.pending_inbound.remove(&pending_id);
                Ok(PairingResponse::Refused {
                    reason: PairingRefuseReason::UserRejected,
                })
            }
        }
    }

    /// 响应入站配对请求（UI 命令调用）
    ///
    /// - `Code` 模式：验证配对码存在且未过期，验证通过后消耗该配对码
    /// - `Direct` 模式：局域网直连授权已由入站校验把关，此处无需再校验
    ///
    /// 返回 `Some(PairedDeviceInfo)` 表示配对已接受并添加到已配对设备。
    pub async fn respond_pairing_request(
        &self,
        pending_id: u64,
        response: PairingResponse,
    ) -> AppResult<Option<PairedDeviceInfo>> {
        let Some((_, pending)) = self.pending_inbound.remove(&pending_id) else {
            return Err(AppError::Network("配对请求已过期或不存在".into()));
        };

        // 仅在接受时校验凭证；拒绝时直接回响应，无需校验。
        //
        // 穷尽 match 而非 `if let`：新增变体不会静默落到免校验通道。
        if matches!(response, PairingResponse::Success) {
            match &pending.method {
                PairingMethod::Code { code } => {
                    let mut guard = self.active_code.lock().unwrap();
                    let info = guard.as_ref().ok_or(AppError::InvalidCode)?;
                    if &info.code != code {
                        return Err(AppError::InvalidCode);
                    }
                    if info.is_expired() {
                        return Err(AppError::ExpiredCode);
                    }
                    *guard = None;
                    // 校验失败时上面提前 return（未 send responder），
                    // pending.responder 随本函数返回被 drop → handler 得 RecvError 婉拒。
                }
                PairingMethod::Direct => {}
            }
        }

        let accepted = matches!(response, PairingResponse::Success);
        // 解决 handler 的 oneshot（send 失败说明 handler 已超时回收，忽略）
        let _ = pending.responder.send(response);

        if !accepted {
            return Ok(None);
        }

        let info = PairedDeviceInfo::new(
            pending.peer_id,
            pending.os_info,
            chrono::Utc::now().timestamp_millis(),
        );
        self.paired_devices.insert(info.peer_id, info.clone());
        Ok(Some(info))
    }

    // === 已配对设备管理 ===

    pub fn is_paired(&self, peer_id: &NodeId) -> bool {
        self.paired_devices.contains_key(peer_id)
    }

    pub fn get_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo> {
        self.paired_devices
            .get(peer_id)
            .map(|entry| entry.value().clone())
    }

    pub fn add_paired_device(&self, info: PairedDeviceInfo) {
        self.paired_devices.insert(info.peer_id, info);
    }

    /// 用 Identify 中收到的最新设备信息刷新已配对设备。
    ///
    /// 返回 `Some` 表示信息已变化，调用方应将其持久化到 host 的 keychain。
    pub fn refresh_paired_device_os_info(
        &self,
        peer_id: &NodeId,
        os_info: OsInfo,
    ) -> Option<PairedDeviceInfo> {
        let mut device = self.paired_devices.get_mut(peer_id)?;
        if !device.refresh_os_info(os_info) {
            return None;
        }
        Some(device.clone())
    }

    pub fn remove_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo> {
        self.paired_devices.remove(peer_id).map(|(_, v)| v)
    }

    pub fn get_paired_devices(&self) -> Vec<PairedDeviceInfo> {
        self.paired_devices
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }
}

/// 配对 typed RPC 服务：把 [`PairingManager`] 适配成 [`RpcService`]。
#[derive(Clone)]
pub struct PairingService(pub Arc<PairingManager>);

impl RpcService<PairingRequest, PairingResponse> for PairingService {
    async fn handle(
        &self,
        from: NodeId,
        req: PairingRequest,
    ) -> Result<PairingResponse, AcceptError> {
        self.0.handle_inbound(from, req).await
    }
}
