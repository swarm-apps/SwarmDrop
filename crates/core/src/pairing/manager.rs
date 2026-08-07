use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use swarmdrop_net::{AcceptError, Addr, CallOptions, Endpoint, NodeId, RpcService};
use tokio::sync::oneshot;

use crate::device::{DeviceName, OsInfo, PairedDeviceInfo};
use crate::device_manager::DeviceManager;
use crate::host::{CoreEvent, EventBus, Notification, Notifier, PairedDeviceStore};
use crate::paired_devices;
use crate::protocol::{
    PAIRING, PairingMethod, PairingRefuseReason, PairingRequest, PairingResponse,
};
use crate::{AppError, AppResult};
use swarmdrop_invite::{
    InviteRegistry, InviteRejectReason, InviteStore, InviteSummary, PairInvite, TransportPolicy,
};

/// 出站配对调用超时（对齐旧栈 req_resp_timeout，容纳对端等用户决策的长交互）。
const PAIRING_CALL_TIMEOUT: Duration = Duration::from_secs(180);

/// 入站配对请求待决表最长等待（超时回收，避免 handler 任务无限挂起）。
///
/// 刻意**小于** [`PAIRING_CALL_TIMEOUT`]（180s）——与 transfer 的
/// `PENDING_OFFER_TIMEOUT_SECS`(170) < `req_resp_timeout`(180) 同款错位：本端 pending 先于
/// 发起端放弃被回收，保证"本端刚接受、发起端已超时放弃 → 回复通道已关"的边界竞态不发生。
const PENDING_INBOUND_TIMEOUT: Duration = Duration::from_secs(170);

/// 当前 Unix 秒（邀请 TTL 判定用；chrono 在 wasm 下走 js 时钟）。
fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

/// 每类网络只保留一条原生路径和一条 WebRTC 路径，避免把全部监听地址复制进邀请。
///
/// 两类路径分别从整类地址中挑选，不能假设同一网卡一定同时监听所有传输；否则先出现的
/// TCP-only 网卡会把后续 WebRTC 地址挤掉，桌面生成的邀请便可能无法从 Web 端打开。
fn select_invite_addrs(addrs: Vec<Addr>, policy: TransportPolicy) -> Vec<Addr> {
    let mut shared = Vec::new();
    let mut lan = Vec::new();
    let mut public = Vec::new();
    let mut benchmark = Vec::new();
    let mut circuits = Vec::new();

    // 五个类别互斥（`is_private_lan` 只认 RFC1918，`is_public_routable` 已排除
    // 100.64/10 与 198.18/15），所以分桶顺序只影响可读性，不影响归属。
    for addr in addrs {
        if addr.is_loopback_or_unspecified() {
            continue;
        }
        let bucket = if addr.is_circuit() {
            &mut circuits
        } else if addr.is_shared_address_space() {
            &mut shared
        } else if addr.is_private_lan() {
            &mut lan
        } else if addr.is_public_routable() {
            &mut public
        } else if addr.is_benchmarking_address() {
            &mut benchmark
        } else {
            continue;
        };
        if !bucket.contains(&addr) {
            bucket.push(addr);
        }
    }

    let mut selected = Vec::new();
    if policy == TransportPolicy::LocalOnly {
        append_invite_transports(&mut selected, &lan);
        return selected;
    }

    append_invite_transports(&mut selected, &shared);
    append_invite_transports(&mut selected, &lan);
    append_invite_transports(&mut selected, &public);
    if shared.is_empty() {
        append_invite_transports(&mut selected, &benchmark);
    }
    append_invite_transports(&mut selected, &circuits);
    selected
}

/// native 优先保留可穿过 UDP 封锁的 TCP（无则 QUIC），浏览器另保留 WebRTC。
fn append_invite_transports(selected: &mut Vec<Addr>, paths: &[Addr]) {
    // `!is_webrtc()` 不是冗余：WebRTC 地址底下压着 tcp/udp 段（circuit 打洞地址
    // `/ip4/…/tcp/…/p2p-circuit/webrtc` 就是），不排除它就会被当 native 选走，
    // 浏览器那一条反而落空。
    let native = paths
        .iter()
        .find(|addr| addr.is_tcp() && !addr.is_webrtc())
        .or_else(|| {
            paths
                .iter()
                .find(|addr| addr.is_quic_v1() && !addr.is_webrtc())
        });
    let browser = paths.iter().find(|addr| addr.is_webrtc());

    // 两类都没命中说明这一类只有别的传输，留一条免得整类地址丢空。
    let picked = match (native, browser) {
        (None, None) => [paths.first(), None],
        pair => [pair.0, pair.1],
    };
    for addr in picked.into_iter().flatten() {
        if !selected.contains(addr) {
            selected.push(addr.clone());
        }
    }
}

/// 一次配对达成的产物：设备信息 + 它有没有落盘。
///
/// **为什么 `persisted` 是返回值而不是错误。** 走到这里时配对已经是**双方共识**
/// —— 对端收到了 `Success` 并把本机写进了它的已配对列表。此刻本机若因写盘失败而向上
/// 报 `Err`，用户看到的是「配对失败」，而对方看到的是「配对成功」：两台设备对同一件事
/// 的认知从此分叉，且没有任何一端会去纠正它。
///
/// 真实后果只有一个 —— **这台设备重启后会从本机列表里消失**（对端仍记着）。UI 该照这个
/// 说，而不是说配对没成。形态与 [`revoke_invite_by_hash`](PairingManager::revoke_invite_by_hash)
/// 的返回值同构：那里也是「本次运行内已生效、但重启后会复活」，同样由 UI 如实告知。
#[derive(Debug, Clone)]
pub struct PairedDeviceCommit {
    /// **合并后**的设备信息（保留用户此前设过的信任级别与收件策略）。
    pub device: PairedDeviceInfo,
    /// `false` = 本次运行内可用，但重启后会丢。
    pub persisted: bool,
}

/// 从一次配对尝试的结果里取「设备有没有落盘」。
///
/// **没配成（`None`）⇒ `true`**。这条约定是反直觉的，所以只在这里写一次：对端拒绝或
/// 婉拒时根本没有「该落盘的东西」，此时报 `false` 会让 UI 弹一句无从解释的警告
/// （「配对成功但没保存」——可它压根没配成）。
///
/// 三端都调它，不要各写一遍 `is_none_or`：真正容易漂的不是那一行，而是这条约定本身，
/// 任一端写成 `map_or(false, ..)` 就会在用户点「拒绝」时弹出那句无解提示。
pub fn persisted_or_absent(commit: Option<&PairedDeviceCommit>) -> bool {
    commit.is_none_or(|commit| commit.persisted)
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
    /// **本机**设备信息快照（平台探测 + 用户设备名），由 `runtime::start_node` 从
    /// `DeviceConfig` 端口装配后注入。配对请求（[`Self::request_pairing`]）与邀请串
    /// （[`Self::encode_invite`]）的 display 都读它。
    ///
    /// **运行时可变**：唯一写口是 `Self::set_device_name`（`pub(crate)`），crate 外只能经
    /// [`NetManager::set_local_device_name`](crate::network::NetManager::set_local_device_name)
    /// ——它同时更新 identify 的 `agent_version`，两者同一时刻生效，不会出现
    /// 「新发的邀请写着新名字、对端 identify 到的还是旧名字」的中间态。
    ///
    /// 用 `std::sync::RwLock` 而非 tokio 的：读者都是「clone 一份就走」，临界区里没有
    /// `.await`。std 的 guard 不是 `Send`，谁把它跨 `.await` 持有，future 立刻不满足
    /// `Send` —— 编译期就红，这是特性不是障碍。
    os_info: RwLock<OsInfo>,
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
    /// 已配对设备列表的持久化端口。**三个写方向都在这里**：新增与刷新走
    /// [`Self::commit_paired_device`]，移除走 [`Self::unpair`]。
    ///
    /// 新增/刷新曾经是三端 host 各自在 `CoreEvent::PairedDeviceAdded` 上回写的（那时这里
    /// 只服务 `unpair`），后果是同一个动作长出三种失败语义：桌面/Web 在命令层 `?` 冒泡
    /// （配对报错，而对端已成功），移动端在 event bus 里只记一条 warn（静默丢失）。
    paired_store: Arc<dyn PairedDeviceStore>,
}

impl PairingManager {
    /// `invite_store` 是邀请注册表的落盘端口（native = SQL / wasm = IndexedDB），
    /// 由宿主在组装点注入；传 [`NoopInviteStore`](swarmdrop_invite::NoopInviteStore)
    /// 退回旧的「重启丢邀请」语义。**构造后需调用 [`Self::load_invites`]** 把已落盘的
    /// 邀请读回内存表，否则重启后它们等同不存在。
    ///
    /// `paired_store` 是已配对设备列表的持久化端口（native = keychain 条目 /
    /// wasm = IndexedDB），[`Self::unpair`] 靠它把解除配对做成原子操作。
    ///
    /// `os_info` 是本机设备信息，由组合根注入——**不要在这里 `OsInfo::default()`**，
    /// 那正是「用户设的名字进不了配对请求、也进不了邀请串」这条 bug 的成因。
    #[expect(
        clippy::too_many_arguments,
        reason = "与 NetManager::new 同源：参数都是三端各自供给的端口/身份/配置"
    )]
    pub fn new(
        endpoint: Endpoint,
        os_info: OsInfo,
        paired_devices: Arc<DashMap<NodeId, PairedDeviceInfo>>,
        devices: Arc<DeviceManager>,
        event_bus: Arc<dyn EventBus>,
        notifier: Option<Arc<dyn Notifier>>,
        invite_store: Arc<dyn InviteStore>,
        paired_store: Arc<dyn PairedDeviceStore>,
    ) -> Self {
        Self {
            endpoint,
            os_info: RwLock::new(os_info),
            paired_devices,
            pending_inbound: DashMap::new(),
            next_pending_id: AtomicU64::new(0),
            devices,
            event_bus,
            notifier,
            invite_registry: InviteRegistry::new(invite_store),
            paired_store,
        }
    }

    /// 启动时把落盘的邀请读回内存表（顺带清掉已过期的）。
    ///
    /// 内存表是一次性消费的权威判定点，不 load 就等于「重启后所有已发出的邀请都不认识」
    /// —— 对方点开链接会得到「邀请无效」。
    pub async fn load_invites(&self) {
        self.invite_registry.load(now_secs()).await;
    }

    /// 本机未过期的已发出邀请（供「已发出邀请」列表与撤销）。
    pub fn list_invites(&self) -> Vec<InviteSummary> {
        self.invite_registry.list_active(now_secs())
    }

    /// 本机可供对端拨号的精简地址集。
    fn shareable_addrs(&self, policy: TransportPolicy) -> Vec<Addr> {
        select_invite_addrs(self.endpoint.watch_addrs().get().dialable(), policy)
    }

    /// 本机 [`OsInfo`] 快照。
    ///
    /// 读锁只活在本函数内——调用方拿到的是 clone，天然不可能把 guard 跨 `.await` 持有。
    pub fn os_info(&self) -> OsInfo {
        self.os_info.read().expect("os_info 锁中毒").clone()
    }

    /// 更新本机设备名，返回更新后的完整 [`OsInfo`]（供调用方重算 `agent_version`）。
    ///
    /// **刻意不提供 `set_os_info(OsInfo)`。** `to_agent_version()` 拼出来的串里还带着
    /// `caps=lan-helper`（由 `runtime::start_node` 按 `provide_lan_helper` 叠加），而对端
    /// 靠它决定要不要把本机登记成 LAN Helper（`network/event_loop.rs` 的
    /// `maybe_register_lan_helper`）。一个整包写口意味着某天有人从别处 new 一个 `OsInfo`
    /// 传进来（比如手边正好有个 `OsInfo::default()`），改一次名就把 capability 抹了——本机从
    /// 别人的 LAN Helper 名单里静默消失，表现是「同网发现忽然变慢了」，几乎不可能定位回
    /// 改名这一步。窄写口让这件事**结构上无法发生**：调用方给不了 capabilities，也给不了
    /// hostname / os / arch。
    ///
    /// 返回完整快照同理：`agent_version` 必须由同一个真值重算，而不是由调用方自己拼。
    ///
    /// **`pub(crate)` 是同一条防线的第二半。** 本方法只改内存态；identify 的
    /// `agent_version` 要另起一步 `endpoint().set_agent_version(...)` 才跟上。谁单独调一次
    /// 本方法，就得到「新发的邀请写新名、对端 identify 读到的还是旧名」的静默偏差——正是
    /// 上面那个窄写口费力气要防的同类问题，只是发生在调用序列上而非参数上。crate 外的
    /// 唯一入口是
    /// [`NetManager::set_local_device_name`](crate::network::NetManager::set_local_device_name)，
    /// 它把两步做成一次。
    pub(crate) fn set_device_name(&self, name: Option<DeviceName>) -> OsInfo {
        let mut os_info = self.os_info.write().expect("os_info 锁中毒");
        os_info.name = name.map(DeviceName::into_string);
        os_info.clone()
    }

    // === 邀请（PairInvite）管理 ===

    /// 生成邀请并返回编码串：签名 + 登记进 [`InviteRegistry`]。不经 DHT——邀请串自包含
    /// 地址提示，靠带外信道（二维码/链接）传递。
    ///
    /// display（名字 + 平台）读本机 [`Self::os_info`] 当下的快照（改名后新发的邀请立即带
    /// 新名字，见 [`Self::set_device_name`]），**不由调用方传入**：三端曾各自
    /// 传一份，桌面与移动传的都是 `OsInfo::default()`，邀请卡上的设备名于是恒为占位主机名。
    pub async fn encode_invite(
        &self,
        secret: &swarmdrop_net::SecretKey,
        policy: TransportPolicy,
    ) -> String {
        let now = now_secs();
        let os_info = self.os_info();
        let invite = PairInvite::generate(
            secret,
            self.shareable_addrs(policy),
            policy,
            os_info.display_name(),
            os_info.platform,
            now,
        );
        // 先落盘再返回串：邀请一旦交到用户手上就可能被立刻使用，注册表里没有它就等于
        // 「不认识」→ 直接拒绝。
        self.invite_registry.register(&invite, now).await;
        invite.encode(secret)
    }

    /// 撤销本机发出的邀请：重新生成覆盖旧串、用户主动放弃、关闭邀请界面时调用。
    ///
    /// **幂等且无副作用**，故不返回 `Result`——邀请串解不开、或它的 capability 不在
    /// 本机 registry 里（已消费 / 非本机发出 / 节点重启后表已空），语义上都等价于
    /// 「它已经不可用了」，正是调用方要的终态。传入他人的邀请串同样是 no-op：
    /// registry 按 `sha256(capability)` 索引，查不到就什么都不做。
    ///
    /// 撤销**不是**过期的替代：TTL 到点自然失效，本方法是让它提前失效。
    ///
    /// 返回**是否已落盘**。`false` 意味着本进程内撤销生效了，但重启后那条邀请会复活
    /// （写穿失败，库里仍是 `register` 写下的 Pending）—— 调用方应当让用户知道，
    /// 而不是让他以为撤销成功了。
    pub async fn revoke_invite(&self, invite_str: &str) -> bool {
        match PairInvite::decode(invite_str) {
            Ok(invite) => self.invite_registry.revoke(&invite.capability).await,
            // 解不开就谈不上撤销，降到 debug——调用方多为 fire-and-forget 的清理路径。
            // 报 true：没有需要持久化的东西。
            Err(e) => {
                tracing::debug!("撤销邀请时解码失败（视作已失效）: {e}");
                true
            }
        }
    }

    /// 按 capability 哈希撤销（邀请列表里只有哈希，没有原串 —— 明文不落盘）。
    /// 返回是否已落盘，见 [`Self::revoke_invite`]。
    pub async fn revoke_invite_by_hash(&self, capability_hash: [u8; 32]) -> bool {
        self.invite_registry.revoke_by_hash(capability_hash).await
    }

    /// 受邀方：解码邀请串 → 验签 → TTL 预检 → 按策略过滤地址 → 连接发起方出示凭证。
    ///
    /// 身份 pin 由 `request_pairing` 内的连接握手强制（连到的必然是 `inviter_id`，
    /// 冒充在密码学上不可能）；LocalOnly 下地址提示已过滤为仅私网。
    pub async fn pair_with_invite(
        &self,
        invite_str: &str,
    ) -> AppResult<(PairingResponse, Option<PairedDeviceCommit>)> {
        let invite = PairInvite::decode(invite_str).map_err(|e| {
            tracing::warn!("邀请解码失败: {e}");
            AppError::InvalidCode
        })?;
        if invite.is_expired(now_secs()) {
            return Err(AppError::ExpiredCode);
        }
        let method = PairingMethod::Invite {
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
    ) -> AppResult<(PairingResponse, Option<PairedDeviceCommit>)> {
        if let Some(addrs) = addrs.filter(|a| !a.is_empty()) {
            self.endpoint
                .add_addrs(peer_id, addrs)
                .await
                .map_err(|e| AppError::Network(format!("注册对端地址失败: {e}")))?;
        }

        let req = PairingRequest {
            // 本机 OsInfo 快照，不是 `OsInfo::default()`——后者不含用户设的设备名，
            // 对端的配对确认弹窗于是恒显示占位主机名（Web 端更是「Device · unknown」）。
            os_info: self.os_info(),
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
                // 落盘 / 内存表 / 事件三件事都在 commit 里，宿主不再各做一遍。
                let commit = self.commit_paired_device(info).await;

                Ok((PairingResponse::Success, Some(commit)))
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

        // Invite：非消费预检——明显非法（未知/过期/已用）直接婉拒，
        // 不打扰用户、不占一次性额度。权威 CAS 消费留到用户确认（respond Success）。
        if let PairingMethod::Invite { capability } = &req.method
            && let Err(reason) = self.invite_registry.check(capability, now_secs())
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
                    // 与 `CoreEvent::PairingRequestReceived` 喂的弹窗同源：那边 UI 走
                    // `display_name()`，这里若取裸 `hostname`，同一个请求会出现
                    // 「弹窗显示用户名、系统通知显示主机名」的自相矛盾。
                    hostname: req.os_info.display_name(),
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
    ) -> AppResult<Option<PairedDeviceCommit>> {
        let Some((_, pending)) = self.pending_inbound.remove(&pending_id) else {
            return Err(AppError::Network("配对请求已过期或不存在".into()));
        };

        // 仅在接受时校验凭证；拒绝时直接回响应，无需校验。
        //
        // 穷尽 match 而非 `if let`：新增变体不会静默落到免校验通道。
        if matches!(response, PairingResponse::Success) {
            match &pending.method {
                PairingMethod::Direct => {}
                PairingMethod::Invite { capability } => {
                    // 权威一次性消费（CAS）：两台设备同时扫同码时，仅先确认者成功，
                    // 后者拿到 Unavailable → 提前 return（未 send responder）→ 对端婉拒。
                    self.invite_registry
                        .try_consume(capability, now_secs())
                        .await
                        .map_err(|reason| match reason {
                            InviteRejectReason::Expired => AppError::ExpiredCode,
                            // 不是「邀请无效」——是本机没能把「已消费」落盘，所以宁可让
                            // 这次配对失败也不能放行（否则重启后同一凭证还能再用一次）。
                            // 这里排在 responder.send(Success) 之前，报失败是诚实的。
                            //
                            // 用**专属 kind** 而不是 `Identity`：文案由前端按 kind 渲染，
                            // 包成 Identity 会让用户在点「接受配对」时看到一句
                            // 「设备身份初始化失败」——与真实原因毫无关系。
                            InviteRejectReason::NotPersisted => AppError::InvitePersistFailed,
                            InviteRejectReason::Unknown | InviteRejectReason::Unavailable => {
                                AppError::InvalidCode
                            }
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
        Ok(Some(self.commit_paired_device(info).await))
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

    /// **新增/刷新一台已配对设备的唯一提交入口**：落盘 → 共享内存表 → 发事件。
    ///
    /// 两个调用方向都走它：配对刚达成（本模块的三个达成点），以及对端经 identify
    /// 广播了新设备名（`network::event_loop`）。此前这三件事散在三端 host 各写一遍，
    /// 于是同一个动作长出了三种失败语义 —— 桌面/Web 在命令层 `?` 冒泡（配对报错，
    /// 而对端已成功）、移动端在 event bus 里只记一条 warn（静默丢失）。
    ///
    /// **落盘失败不阻断**，返回 `false`（理由见 [`PairedDeviceCommit`]）。
    ///
    /// 三个动作的顺序是有讲究的：
    ///
    /// 1. **先 upsert 拿到合并后的版本。** [`paired_devices::upsert`] 对已存在的条目只刷新
    ///    `os_info` / `paired_at`，**保留用户设过的 `trust_level` / `receive_policy`**。
    ///    配对达成那条路径交上来的 `device` 是 `PairedDeviceInfo::new` 的产物（默认策略），
    ///    直接拿它整条替换，就是对「重新配对一台已配对设备」做了一次静默的策略重置 ——
    ///    而 `receive_policy` 是被 `swarmdrop_transfer::policy` 真正裁决的，不是展示字段。
    ///    （identify 刷新那条交的是内存表里的版本，本来就带着完整策略；两条都走同一套
    ///    合并规则，不必分辨来源。）
    /// 2. **再写内存表**，写的是合并后的版本（同一理由）。内存表是 `is_paired` /
    ///    presence 白名单 / 入站 offer 裁决的事实源，它和库里那份必须是同一个东西。
    /// 3. **最后发事件**，无论落盘成没成 —— 设备在本次运行内确实可用，UI 必须看得见它。
    pub(crate) async fn commit_paired_device(
        &self,
        device: PairedDeviceInfo,
    ) -> PairedDeviceCommit {
        let peer_id = device.peer_id;

        // **先写内存表，再落盘。** `respond_pairing_request` 在此之前已经把 `Success`
        // 回给了对端，而落盘是一次真实 I/O（桌面钥匙串 / Web IndexedDB 往返 / 移动平台
        // 安全存储）—— 那段窗口里对端可能已经发来 offer，而本机 `is_paired` 还是 false，
        // 会被 `NotPaired` 拒掉。写在 await 之前，这条缝就不存在。
        //
        // 写进去的是**合并结果**而不是入参：入参恒是 `PairedDeviceInfo::new` 的产物
        // （`Collaborator` + 默认收件策略），而这张表正是 `swarmdrop_transfer::policy`
        // 裁决入站 offer 的事实源 —— 直接盖上去会把用户设的 `Owned` 降回 `Collaborator`、
        // 把收紧过的策略放回默认。合并规则与 `upsert` 共用同一份（`merge_observation`）。
        let optimistic = self.merge_into_memory(peer_id, device.clone());
        self.paired_devices.insert(peer_id, optimistic.clone());

        let (merged, persisted) = match paired_devices::upsert(&*self.paired_store, device).await {
            Ok(merged) => {
                // 库里那份是权威合并结果（它见过完整的历史条目），覆盖乐观写入。
                self.paired_devices.insert(peer_id, merged.clone());
                (merged, true)
            }
            Err(error) => {
                tracing::warn!("持久化已配对设备失败（本次运行内仍可用，重启后会丢）: {error}");
                (optimistic, false)
            }
        };

        let _ = self
            .event_bus
            .publish(CoreEvent::PairedDeviceAdded {
                device: merged.clone(),
            })
            .await;
        PairedDeviceCommit {
            device: merged,
            persisted,
        }
    }

    /// identify 刷新专用的提交入口：**设备仍在已配对表里才提交**。
    ///
    /// 刷新语义本来就不该带「新增」。不做这个复检的话，「用户点了解除配对」与「identify
    /// 刷新到达」交错时（两者之间隔着 `event_loop` 的
    /// `publish_devices_and_status(..).await`，窗口是真实存在的），刚解除的设备会被
    /// `upsert` 重新 push 回持久化列表、insert 回内存表，还会发一条 `PairedDeviceAdded`
    /// —— presence 继续保活、入站 offer 重新受理、UI 上它又冒出来。
    ///
    /// **残留窗口**：复检与随后的 insert 之间仍可能被 `unpair` 插入（`paired_store` 这一层
    /// 没有写锁，read-modify-write 不是原子的）。彻底根治要给端口加锁 —— 收口之后那只需
    /// 改一处。复检把窗口从「两次事件发布 + 一次落盘 I/O」缩到几条指令。
    pub(crate) async fn refresh_paired_device(&self, device: PairedDeviceInfo) {
        if !self.paired_devices.contains_key(&device.peer_id) {
            tracing::debug!(
                "identify 刷新到达时设备已解除配对，忽略: {}",
                device.peer_id
            );
            return;
        }
        self.commit_paired_device(device).await;
    }

    /// 把一次新观测合进共享内存表里已有的那条（若有），返回合并结果。
    ///
    /// 只用在 [`Self::commit_paired_device`] 的**落盘失败回退**上 —— 成功路径的合并结果
    /// 由 `paired_devices::upsert` 给出。**这里不写表**：写表统一由 commit 做，
    /// 免得同一个 peer 在两处各插一次。
    fn merge_into_memory(&self, peer_id: NodeId, observed: PairedDeviceInfo) -> PairedDeviceInfo {
        // `get` 的 Ref 先 clone 出来再放手：DashMap 的读锁跨到随后的 `insert` 会死锁。
        match self.paired_devices.get(&peer_id).map(|entry| entry.clone()) {
            Some(mut existing) => {
                existing.merge_observation(observed);
                existing
            }
            None => observed,
        }
    }

    /// 用 Identify 中收到的最新设备信息刷新已配对设备。
    ///
    /// 返回 `Some` 表示信息已变化，调用方应把它交给 [`Self::commit_paired_device`]
    /// ——**不要自己 upsert**，那正是这条路径此前散在三端 host 里的做法。
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

    /// 解除配对：持久化 → 共享内存表 → 事件，三个副作用一次做完，返回更新后的列表。
    ///
    /// **顺序是 fail-closed 的，不能对调。** 先写盘、失败即整体 `Err` 且内存表原样保留，
    /// 用户看到的是「操作失败、设备还在」，重试即可、两次运行的状态一致；反过来（先删
    /// 内存后写盘，桌面此前的做法）在持久化失败时是「本次运行里设备消失了、用户以为成功，
    /// 重启后它又回来了」——「本次运行生效、重启后失效」是最坏的一种成功。同一准则在配对
    /// 接受路径上已经用过一次：[`Self::respond_pairing_request`] 对
    /// `InviteRejectReason::NotPersisted` 宁可让配对失败也不放行。
    ///
    /// **删 `DashMap` 是 presence 撤销的唯一开关。**
    /// [`PresenceSupervisor`](crate::presence::PresenceSupervisor) 的 `reconcile_whitelist`
    /// 算的是 `presence − paired` 差集，`paired` 就是这份共享内存表；只删持久化的话保活与
    /// 重探会一直跑到进程退出（撤销靠 1s tick 收敛，不是同步的）。同一份表也是
    /// `is_paired` / `DeviceManager` / `PeerDirectory` 的事实源，删它一处即全部收敛。
    ///
    /// **宿主不直接调本方法**，调 [`paired_devices::unpair`]——它按节点是否在跑分派，
    /// 节点没跑时（拿不到 `PairingManager`）走 [`paired_devices::remove`] 并补发事件。
    ///
    /// 幂等：内存与持久化列表都不含该 peer 时是 no-op，**不发事件**（保持
    /// 「事件 == 集合真的变了」这个不变量，避免下游把重复点击当成两次变更），
    /// 返回当前列表。
    ///
    /// 新增/刷新方向见 [`Self::commit_paired_device`]，两个方向现在都在 core 里
    /// （host 只转发事件，不再回写）。
    pub async fn unpair(&self, peer_id: &NodeId) -> AppResult<Vec<PairedDeviceInfo>> {
        let devices = self.paired_store.load_paired_devices().await?;
        let persisted = devices.iter().any(|item| &item.peer_id == peer_id);
        let in_memory = self.paired_devices.contains_key(peer_id);
        if !persisted && !in_memory {
            return Ok(devices);
        }

        // ① 持久化：失败即整体报错，内存表不动。retain + save 那段算法不在这里重写一遍。
        let devices = if persisted {
            paired_devices::remove(&*self.paired_store, peer_id).await?
        } else {
            devices
        };
        // ② 共享内存表：presence 撤销与 is_paired 的唯一开关
        self.paired_devices.remove(peer_id);
        // ③ 通知 host（不再重复删持久化）
        let _ = self
            .event_bus
            .publish(CoreEvent::PairedDeviceRemoved { peer_id: *peer_id })
            .await;

        Ok(devices)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use swarmdrop_net::{Router, SecretKey};

    use super::*;
    use crate::device::{DeviceTrustLevel, OsInfo};
    use crate::host::{MemoryHost, PairedDeviceStore};
    use crate::presence::PresenceMap;
    use crate::protocol::PAIRING_PROTOCOL;
    use swarmdrop_invite::NoopInviteStore;

    /// 注入用的本机设备名。刻意与 `hostname` 不同——相同的话「读的是 name 还是 hostname」
    /// 这两种实现都能让断言通过，测试就钉不住东西了。
    const INJECTED_NAME: &str = "书房 Mac";

    fn addr(value: &str) -> Addr {
        value.parse().unwrap()
    }

    fn memory_host() -> MemoryHost {
        MemoryHost::new()
    }

    /// 组合根装配好的本机 `OsInfo`（平台探测 + 用户设备名）的替身。
    fn injected_os_info() -> OsInfo {
        OsInfo {
            name: Some(INJECTED_NAME.to_string()),
            hostname: "raw-hostname".to_string(),
            os: "macos".to_string(),
            platform: "macos".to_string(),
            arch: "aarch64".to_string(),
            capabilities: Vec::new(),
        }
    }

    /// 关 mDNS / relay client / 不监听：unpair 只碰内存表与端口，不需要真实链路。
    async fn test_endpoint() -> Endpoint {
        test_endpoint_with(SecretKey::generate()).await
    }

    async fn test_endpoint_with(secret: SecretKey) -> Endpoint {
        Endpoint::builder()
            .secret_key(secret)
            .mdns(false)
            .relay_client(false)
            .bind()
            .await
            .expect("bind test endpoint")
    }

    async fn test_manager(
        paired: Arc<DashMap<NodeId, PairedDeviceInfo>>,
        event_bus: Arc<dyn EventBus>,
        paired_store: Arc<dyn PairedDeviceStore>,
    ) -> PairingManager {
        let presence: PresenceMap = Arc::new(DashMap::new());
        let devices = Arc::new(DeviceManager::new(paired.clone(), presence));
        PairingManager::new(
            test_endpoint().await,
            OsInfo::default(),
            paired,
            devices,
            event_bus,
            None,
            Arc::new(NoopInviteStore),
            paired_store,
        )
    }

    /// 直接构造一个注入了已知 `OsInfo` 的 `PairingManager`。
    ///
    /// **刻意不经 `runtime::start_node`**：core 的 e2e 全都手抄了它的 body
    /// （`tests/e2e_transfer.rs` 里写得很直白），那条组合根路径在 core 内没有 harness，
    /// 给它造一套只为验证一行赋值不划算。本函数覆盖的是「注入 → 三条对外表示」这一段。
    fn manager_with_os_info(endpoint: Endpoint, os_info: OsInfo) -> PairingManager {
        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        let presence: PresenceMap = Arc::new(DashMap::new());
        let devices = Arc::new(DeviceManager::new(paired.clone(), presence));
        let host = memory_host();
        PairingManager::new(
            endpoint,
            os_info,
            paired,
            devices,
            Arc::new(host.clone()),
            None,
            Arc::new(NoopInviteStore),
            Arc::new(host),
        )
    }

    fn paired_device(peer_id: NodeId) -> PairedDeviceInfo {
        PairedDeviceInfo::new(peer_id, OsInfo::default(), 1)
    }

    /// 只在 `save_paired_devices` 报错的测试替身：验证 fail-closed 顺序。
    struct FailingStore {
        devices: Mutex<Vec<PairedDeviceInfo>>,
    }

    #[async_trait]
    impl PairedDeviceStore for FailingStore {
        async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
            Ok(self.devices.lock().expect("store poisoned").clone())
        }

        async fn save_paired_devices(&self, _devices: &[PairedDeviceInfo]) -> AppResult<()> {
            // 存储 I/O 失败，**不是**身份错误 —— `Identity` 不该再当通用垃圾桶用
            // （判据见 `crates/host/src/error.rs` 上的说明）。
            Err(AppError::Io(std::io::Error::other("保存已配对设备失败")))
        }
    }

    /// 落盘失败时，共享内存表里**用户设过的策略不能被默认值冲掉**。
    ///
    /// 走到 commit 时手里的 `device` 恒是 [`PairedDeviceInfo::new`] 的产物
    /// （`Collaborator` + 默认收件策略）。直接把它 `insert` 进 DashMap 的话，一次写库失败
    /// 就会让用户设的 `Owned` 降回 `Collaborator`、把收紧过的收件策略放回默认，
    /// 而那张表正是 `swarmdrop_transfer::policy` 裁决入站 offer 的事实源 ——
    /// **本次运行内立即生效**。这条红了说明失败分支又在拿未合并的观测覆盖内存表。
    #[tokio::test]
    async fn commit_keeps_user_policy_in_memory_when_persist_fails() {
        let peer_id = SecretKey::generate().node_id();

        let mut owned = paired_device(peer_id);
        owned.trust_level = DeviceTrustLevel::Owned;
        owned.receive_policy.auto_accept = false;
        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        paired.insert(peer_id, owned);

        let store = Arc::new(FailingStore {
            devices: Mutex::new(Vec::new()),
        });
        let manager = test_manager(paired.clone(), Arc::new(memory_host()), store).await;

        // 再走一次配对：回调交上来的是默认策略的新观测。
        let observed = PairedDeviceInfo::new(peer_id, OsInfo::default(), 99);
        let commit = manager.commit_paired_device(observed).await;

        assert!(!commit.persisted, "写库失败必须如实报 false，不能压成成功");
        let in_memory = paired.get(&peer_id).expect("设备仍在内存表").clone();
        assert_eq!(
            in_memory.trust_level,
            DeviceTrustLevel::Owned,
            "用户设的信任级别不能被默认值冲掉"
        );
        assert!(
            !in_memory.receive_policy.auto_accept,
            "收紧过的收件策略同理"
        );
        assert_eq!(in_memory.paired_at, 99, "os_info / paired_at 仍该被刷新");
    }

    /// identify 刷新**不得让已解除配对的设备复活**。
    ///
    /// 时序：刷新事件在途 → 用户点解除配对（`unpair` 删库 + 删内存表）→ 刷新落到
    /// `refresh_paired_device`。若它直接 commit，`upsert` 会把设备 push 回持久化列表、
    /// insert 回内存表，presence 继续保活、入站 offer 重新受理、UI 上它又冒出来。
    #[tokio::test]
    async fn refresh_does_not_resurrect_an_unpaired_device() {
        let host = memory_host();
        let peer_id = SecretKey::generate().node_id();

        // 内存表与库都已不含该 peer（= 刚被 unpair 过）。
        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        let manager = test_manager(
            paired.clone(),
            Arc::new(host.clone()),
            Arc::new(host.clone()),
        )
        .await;

        manager.refresh_paired_device(paired_device(peer_id)).await;

        assert!(paired.is_empty(), "刷新不该把已解除配对的设备写回内存表");
        assert!(
            host.load_paired_devices().await.unwrap().is_empty(),
            "更不该写回持久化列表"
        );
    }

    /// 成功路径：落盘、内存表、返回值三者拿到的都是**合并后**的版本。
    #[tokio::test]
    async fn commit_persists_and_publishes_merged_device() {
        let host = memory_host();
        let peer_id = SecretKey::generate().node_id();

        let mut owned = paired_device(peer_id);
        owned.trust_level = DeviceTrustLevel::Owned;
        host.save_paired_devices(std::slice::from_ref(&owned))
            .await
            .expect("seed store");

        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        paired.insert(peer_id, owned);
        let manager = test_manager(
            paired.clone(),
            Arc::new(host.clone()),
            Arc::new(host.clone()),
        )
        .await;

        let observed = PairedDeviceInfo::new(peer_id, OsInfo::default(), 77);
        let commit = manager.commit_paired_device(observed).await;

        assert!(commit.persisted);
        assert_eq!(commit.device.trust_level, DeviceTrustLevel::Owned);
        assert_eq!(commit.device.paired_at, 77);
        assert_eq!(
            paired.get(&peer_id).expect("in memory").trust_level,
            DeviceTrustLevel::Owned
        );
        let stored = host.load_paired_devices().await.expect("load");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].trust_level, DeviceTrustLevel::Owned);
        assert_eq!(stored[0].paired_at, 77);
    }

    #[tokio::test]
    async fn unpair_should_persist_forget_and_publish() {
        let host = memory_host();
        let peer_id = SecretKey::generate().node_id();
        let device = paired_device(peer_id);
        host.save_paired_devices(std::slice::from_ref(&device))
            .await
            .unwrap();

        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        paired.insert(peer_id, device);
        let manager = test_manager(
            paired.clone(),
            Arc::new(host.clone()),
            Arc::new(host.clone()),
        )
        .await;

        let devices = manager.unpair(&peer_id).await.expect("unpair");

        assert!(devices.is_empty());
        assert!(host.load_paired_devices().await.unwrap().is_empty());
        assert!(!paired.contains_key(&peer_id));
        assert!(host.events().iter().any(|event| matches!(
            event,
            CoreEvent::PairedDeviceRemoved { peer_id: removed } if removed == &peer_id
        )));
    }

    #[tokio::test]
    async fn unpair_should_be_idempotent_without_event() {
        let host = memory_host();
        let peer_id = SecretKey::generate().node_id();
        let manager = test_manager(
            Arc::new(DashMap::new()),
            Arc::new(host.clone()),
            Arc::new(host.clone()),
        )
        .await;

        let devices = manager.unpair(&peer_id).await.expect("unpair");

        assert!(devices.is_empty());
        assert!(
            host.events().is_empty(),
            "两处都不含该 peer 时是 no-op，不能发事件"
        );
    }

    #[tokio::test]
    async fn unpair_should_keep_memory_when_persist_fails() {
        let host = memory_host();
        let peer_id = SecretKey::generate().node_id();
        let device = paired_device(peer_id);
        let store = Arc::new(FailingStore {
            devices: Mutex::new(vec![device.clone()]),
        });

        let paired: Arc<DashMap<NodeId, PairedDeviceInfo>> = Arc::new(DashMap::new());
        paired.insert(peer_id, device);
        let manager = test_manager(paired.clone(), Arc::new(host.clone()), store).await;

        assert!(manager.unpair(&peer_id).await.is_err());
        assert!(
            paired.contains_key(&peer_id),
            "持久化失败时内存表必须原样保留，否则重启后设备复活"
        );
        assert!(host.events().is_empty());
    }

    /// 只把入站 `PairingRequest` 抄下来再婉拒——本测试关心的是请求里带了什么，
    /// 不是对端怎么决策。
    #[derive(Clone)]
    struct CapturingPairing(Arc<Mutex<Option<PairingRequest>>>);

    impl RpcService<PairingRequest, PairingResponse> for CapturingPairing {
        async fn handle(
            &self,
            _from: NodeId,
            req: PairingRequest,
        ) -> Result<PairingResponse, AcceptError> {
            *self.0.lock().expect("captured poisoned") = Some(req);
            Ok(PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            })
        }
    }

    /// 等监听地址回填（端口 0 由 OS 分配）。
    async fn wait_listen_addr(endpoint: &Endpoint) -> Addr {
        for _ in 0..200 {
            if let Some(addr) = endpoint.watch_addrs().get().listen.into_iter().next() {
                return addr;
            }
            n0_future::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("监听地址未在超时内就绪");
    }

    /// 回归锚点：`request_pairing` 发出的 `PairingRequest` 必须带本机 `OsInfo`。
    ///
    /// 它此前发的是 `OsInfo::default()`，于是对端的配对确认弹窗恒显示占位主机名
    /// （Web 端发起时更是「Device · unknown」）。这条红了说明本机 OsInfo 又被丢掉了。
    #[tokio::test]
    async fn request_pairing_should_carry_injected_device_name() {
        let responder_secret = SecretKey::generate();
        let responder_id = responder_secret.node_id();
        let responder = Endpoint::builder()
            .secret_key(responder_secret)
            .listen(vec![addr("/ip4/127.0.0.1/tcp/0")])
            .mdns(false)
            .relay_client(false)
            .bind()
            .await
            .expect("bind responder endpoint");

        let captured: Arc<Mutex<Option<PairingRequest>>> = Arc::new(Mutex::new(None));
        // 保活：drop 后入站流路由停止。
        let _router: Router = Router::builder(responder.clone())
            .accept(
                PAIRING_PROTOCOL,
                PAIRING.handler(CapturingPairing(captured.clone())),
            )
            .spawn();
        let responder_addr = wait_listen_addr(&responder).await;

        let manager = manager_with_os_info(test_endpoint().await, injected_os_info());
        let (response, paired) = manager
            .request_pairing(
                responder_id,
                PairingMethod::Direct,
                Some(vec![responder_addr]),
            )
            .await
            .expect("发起配对请求");

        assert!(matches!(response, PairingResponse::Refused { .. }));
        assert!(paired.is_none());

        let req = captured
            .lock()
            .expect("captured poisoned")
            .clone()
            .expect("对端应收到配对请求");
        assert_eq!(req.os_info.name.as_deref(), Some(INJECTED_NAME));
        assert_eq!(req.os_info.display_name(), INJECTED_NAME);
    }

    /// 回归锚点：邀请串的 `display_name` 必须来自本机 `OsInfo`。
    ///
    /// 桌面与移动此前各自传一份 `OsInfo::default()` 进 `encode_invite`，邀请卡上的设备名
    /// 于是恒为占位主机名。参数删掉之后这条路径在类型层面就传不错了，本测试钉住取值来源。
    #[tokio::test]
    async fn encode_invite_should_use_injected_device_name() {
        let secret = SecretKey::generate();
        let endpoint = test_endpoint_with(secret.clone()).await;
        let manager = manager_with_os_info(endpoint, injected_os_info());

        let encoded = manager.encode_invite(&secret, TransportPolicy::Auto).await;
        let invite = PairInvite::decode(&encoded).expect("解码本机刚生成的邀请");

        assert_eq!(invite.display_name, INJECTED_NAME);
        assert_eq!(invite.display_platform, "macos");
    }

    /// 回归锚点：改名**不得**动 `name` 以外的任何字段，尤其是 `capabilities`。
    ///
    /// 这条红了，意味着改一次名就会把 `caps=lan-helper` 从 `agent_version` 里抹掉，对端于是
    /// 不再把本机登记成 LAN Helper（`network/event_loop.rs` 的 `maybe_register_lan_helper`
    /// 提前返回）。表现只是「同网发现忽然变慢了」，几乎不可能定位回改名这一步——写口窄到
    /// 只收 `Option<DeviceName>` 就是为了让它在结构上不可能发生。
    #[tokio::test]
    async fn set_device_name_only_touches_the_name_field() {
        let os_info = injected_os_info().with_capability(OsInfo::LAN_HELPER_CAPABILITY);
        let manager = manager_with_os_info(test_endpoint().await, os_info);

        let updated = manager.set_device_name(DeviceName::parse("客厅 Mac"));

        assert_eq!(updated.name.as_deref(), Some("客厅 Mac"));
        assert_eq!(updated.hostname, "raw-hostname");
        assert!(
            updated
                .to_agent_version()
                .contains(&format!("caps={}", OsInfo::LAN_HELPER_CAPABILITY)),
            "改名后重算的 agent_version 必须仍带 lan-helper capability"
        );
        assert_eq!(
            manager.os_info(),
            updated,
            "返回的快照与管理器内部真值必须同源——agent_version 由它重算"
        );

        // 清空同样只动 name：回落 hostname 是 display_name 的事，不是抹字段。
        let cleared = manager.set_device_name(None);
        assert!(cleared.name.is_none());
        assert_eq!(cleared.display_name(), "raw-hostname");
        assert!(cleared.has_capability(OsInfo::LAN_HELPER_CAPABILITY));
    }

    #[test]
    fn auto_invite_keeps_bounded_direct_and_relay_paths() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let circuit_tcp = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit");
        let circuit_quic = format!("/ip4/203.0.113.9/udp/4001/quic-v1/p2p/{relay_id}/p2p-circuit");
        let circuit_webrtc = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit/webrtc");
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/127.0.0.1/tcp/4001"),
                addr("/ip4/100.100.200.77/tcp/4001"),
                addr("/ip4/100.100.200.77/udp/4001/quic-v1"),
                addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/192.168.1.10/udp/4001/quic-v1"),
                addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
                addr("/ip4/198.51.100.10/tcp/4001"),
                addr("/ip4/198.51.100.10/udp/4001/quic-v1"),
                addr("/ip4/198.51.100.10/udp/4002/webrtc-direct"),
                addr("/ip4/198.18.0.1/udp/4001/quic-v1"),
                addr(&circuit_webrtc),
                addr(&circuit_quic),
                addr(&circuit_tcp),
            ],
            TransportPolicy::Auto,
        );
        let expected = vec![
            addr("/ip4/100.100.200.77/tcp/4001"),
            addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
            addr("/ip4/192.168.1.10/tcp/4001"),
            addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
            addr("/ip4/198.51.100.10/tcp/4001"),
            addr("/ip4/198.51.100.10/udp/4002/webrtc-direct"),
            addr(&circuit_tcp),
            addr(&circuit_webrtc),
        ];

        assert_eq!(selected, expected);
    }

    #[test]
    fn auto_invite_uses_benchmark_address_only_without_shared_overlay() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/198.18.0.1/tcp/4001"),
                addr("/ip4/198.18.0.1/udp/4001/quic-v1"),
            ],
            TransportPolicy::Auto,
        );

        assert_eq!(selected, vec![addr("/ip4/198.18.0.1/tcp/4001")]);
    }

    #[test]
    fn local_only_invite_keeps_only_bounded_lan_paths() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/127.0.0.1/tcp/4001"),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/10.0.0.2/udp/4002/webrtc-direct"),
                addr("/ip4/172.16.0.3/tcp/4001"),
                addr("/ip4/198.51.100.10/tcp/4001"),
            ],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(
            selected,
            vec![
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/10.0.0.2/udp/4002/webrtc-direct"),
            ]
        );
    }

    /// 兜底分支：某一类只剩本函数不认识的传输时，仍要留一条，不能把整类丢空。
    /// 当前 transport 栈（TCP / QUIC / WebRTC）走不到这里，它是新增传输时的安全网。
    #[test]
    fn unknown_transport_class_still_keeps_one_path() {
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/192.168.1.10/udp/4001"),
                addr("/ip4/192.168.1.11/udp/4002"),
            ],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(selected, vec![addr("/ip4/192.168.1.10/udp/4001")]);
    }
}
