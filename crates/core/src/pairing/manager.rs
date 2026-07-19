use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use swarmdrop_net::{AcceptError, Addr, CallOptions, Endpoint, NodeId, RpcService};
use tokio::sync::oneshot;

use crate::device::{OsInfo, PairedDeviceInfo};
use crate::device_manager::DeviceManager;
use crate::host::{CoreEvent, EventBus, Notification, Notifier};
use crate::protocol::{
    PAIRING, PairingMethod, PairingRefuseReason, PairingRequest, PairingResponse,
};
use crate::{AppError, AppResult};
use swarmdrop_invite::{InviteRegistry, InviteRejectReason, PairInvite, TransportPolicy};

/// 出站配对调用超时（对齐旧栈 req_resp_timeout，容纳对端等用户决策的长交互）。
const PAIRING_CALL_TIMEOUT: Duration = Duration::from_secs(180);

/// 入站配对请求待决表最长等待（超时回收，避免 handler 任务无限挂起）。
const PENDING_INBOUND_TIMEOUT: Duration = Duration::from_secs(180);

/// 当前 Unix 秒（邀请 TTL 判定用；chrono 在 wasm 下走 js 时钟）。
fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

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
/// 管理邀请生成/消费、出站配对请求、入站请求的用户决策编排，以及已配对设备的
/// 增删查。在线宣告与已配对设备的 presence 维持见 [`crate::presence`]。
pub struct PairingManager {
    endpoint: Endpoint,
    /// 已配对设备（与 DeviceManager 共享读取）
    paired_devices: Arc<DashMap<NodeId, PairedDeviceInfo>>,
    /// 入站请求待决表（correlation id → 上下文 + oneshot sender）
    pending_inbound: DashMap<u64, PendingInbound>,
    /// correlation id 分配器（进程内自增；不再是旧内核 pending 响应 id）
    next_pending_id: AtomicU64,
    /// Direct 配对的局域网校验依据（`is_lan_discovered`）
    devices: Arc<DeviceManager>,
    /// 入站请求到达时发 [`CoreEvent::PairingRequestReceived`]
    event_bus: Arc<dyn EventBus>,
    /// 入站请求到达时的系统通知（桌面端；移动端传 None）
    notifier: Option<Arc<dyn Notifier>>,
    /// 一次性邀请状态表（发起端：TTL + capability 哈希 + CAS 消费）
    invite_registry: InviteRegistry,
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
            paired_devices,
            pending_inbound: DashMap::new(),
            next_pending_id: AtomicU64::new(0),
            devices,
            event_bus,
            notifier,
            invite_registry: InviteRegistry::new(),
        }
    }

    /// 本机可供对端拨号的地址（监听 ∪ 外部确认地址，去重）。
    fn shareable_addrs(&self) -> Vec<Addr> {
        self.endpoint.watch_addrs().get().dialable()
    }

    // === 邀请（PairInvite）管理 ===

    /// 生成一次性签名邀请并登记（返回领域对象；编码串由调用方 `invite.encode(secret)`
    /// 得到，或用 [`encode_invite`](Self::encode_invite)）。不经 DHT——邀请串自包含
    /// 地址提示，靠带外信道（二维码/链接）传递。
    pub fn generate_invite(
        &self,
        secret: &swarmdrop_net::SecretKey,
        policy: TransportPolicy,
        display: &OsInfo,
    ) -> PairInvite {
        let invite = PairInvite::generate(
            secret,
            self.shareable_addrs(),
            policy,
            display
                .name
                .clone()
                .unwrap_or_else(|| display.hostname.clone()),
            display.platform.clone(),
            now_secs(),
        );
        self.invite_registry.register(&invite);
        invite
    }

    /// 生成邀请并直接返回编码串（[`generate_invite`](Self::generate_invite) + 签名编码）。
    pub fn encode_invite(
        &self,
        secret: &swarmdrop_net::SecretKey,
        policy: TransportPolicy,
        display: &OsInfo,
    ) -> String {
        self.generate_invite(secret, policy, display).encode(secret)
    }

    /// 撤销邀请（用户取消 / 界面关闭）。
    pub fn revoke_invite(&self, invite_id: &[u8; 16]) {
        self.invite_registry.revoke(invite_id);
    }

    /// 受邀方：解码邀请串 → 验签 → TTL 预检 → 按策略过滤地址 → 连接发起方出示凭证。
    ///
    /// 身份 pin 由 `request_pairing` 内的连接握手强制（连到的必然是 `inviter_id`，
    /// 冒充在密码学上不可能）；LocalOnly 下地址提示已过滤为仅私网。
    pub async fn pair_with_invite(
        &self,
        invite_str: &str,
    ) -> AppResult<(PairingResponse, Option<PairedDeviceInfo>)> {
        let invite = PairInvite::decode(invite_str).map_err(|e| {
            tracing::warn!("邀请解码失败: {e}");
            AppError::InvalidCode
        })?;
        if invite.is_expired(now_secs()) {
            return Err(AppError::ExpiredCode);
        }
        let method = PairingMethod::Invite {
            invite_id: invite.invite_id,
            capability: invite.capability,
        };
        self.request_pairing(invite.inviter.id, method, Some(invite.usable_addrs()))
            .await
    }

    // === 配对流程 ===

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
                // OsInfo 用占位值，随后由 identify 交换经 refresh_paired_device_os_info 补全
                // （邀请里的 display_hint 只供确认界面，不作为持久设备信息来源）。
                let os_info = OsInfo::unknown_from_peer_id(&peer_id);
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

        // Invite：非消费预检——明显非法（未知/过期/错 capability/已用）直接婉拒，
        // 不打扰用户、不占一次性额度。权威 CAS 消费留到用户确认（respond Success）。
        if let PairingMethod::Invite {
            invite_id,
            capability,
        } = &req.method
            && let Err(reason) = self
                .invite_registry
                .check(invite_id, capability, now_secs())
        {
            tracing::warn!("拒绝非法邀请配对请求 {from}: {reason:?}");
            return Ok(PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            });
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
        match n0_future::time::timeout(PENDING_INBOUND_TIMEOUT, rx).await {
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
                PairingMethod::Direct => {}
                PairingMethod::Invite {
                    invite_id,
                    capability,
                } => {
                    // 权威一次性消费（CAS）：两台设备同时扫同码时，仅先确认者成功，
                    // 后者拿到 Unavailable → 提前 return（未 send responder）→ 对端婉拒。
                    self.invite_registry
                        .try_consume(invite_id, capability, pending.peer_id, now_secs())
                        .map_err(|reason| match reason {
                            InviteRejectReason::Expired => AppError::ExpiredCode,
                            _ => AppError::InvalidCode,
                        })?;
                }
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

/// transfer 的 [`PeerDirectory`] 端口实现：把 offer 的中继自动接受策略所需的「查已配对
/// 设备」委托给同名 inherent 方法（trait 与 inherent 同名，inherent 优先，无递归）。
impl crate::transfer::peer::PeerDirectory for PairingManager {
    fn get_paired_device(&self, peer_id: &NodeId) -> Option<PairedDeviceInfo> {
        PairingManager::get_paired_device(self, peer_id)
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
