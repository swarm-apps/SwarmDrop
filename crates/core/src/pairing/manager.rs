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

/// 每类网络只保留一条原生路径、一条 WebTransport 路径和一条 WebRTC 路径，
/// 避免把全部监听地址复制进邀请。
///
/// 三类路径分别从整类地址中挑选，不能假设同一网卡一定同时监听所有传输；否则先出现的
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

/// native 优先保留可穿过 UDP 封锁的 TCP（无则 QUIC），浏览器另保留 WebTransport 与 WebRTC。
///
/// # 浏览器要两条，不是一条
///
/// 两者**都是**浏览器可拨的直连传输，且互不能替代：
///
/// - WebTransport 快得多（回环 4.5x，见 CLAUDE.md 的实测），但 Safari < 18.2 /
///   Firefox < 114 / 老 WebView **没有这个 API**（`transport.rs` 的
///   `browser_supports_webtransport` 就是为此存在的）。只给它，那些浏览器一条都拨不动。
/// - WebRTC 还独占**打洞**能力（circuit 段之后的 `/webrtc`），WebTransport 至今没有
///   对应机制。
///
/// 所以是追加而不是替换。代价是邀请 wire 变长（每桶约 +85B，两个 certhash 占大头），
/// 由 [`fit_invite_to_scannable`] 按码面密度回收 —— 地址多到扫不动时 WebTransport
/// 是第一批被丢的，见那里的丢弃顺序。
fn append_invite_transports(selected: &mut Vec<Addr>, paths: &[Addr]) {
    // `!is_webrtc()` 不是冗余：WebRTC 地址底下压着 tcp/udp 段（circuit 打洞地址
    // `/ip4/…/tcp/…/p2p-circuit/webrtc` 就是），不排除它就会被当 native 选走，
    // 浏览器那一条反而落空。
    //
    // `!is_webtransport()` 同理且更隐蔽：WebTransport 地址里有 `/quic-v1` 段，
    // `is_quic_v1()` 对它为真（见 `Addr::is_webtransport` 的文档）。不排除的话，
    // 一张没有裸 QUIC 的网卡上 native 会挑走 WebTransport —— 下面那条本来要给它的位置
    // 于是变成重复，白白丢掉一条真正的 native 路径。
    let native = paths
        .iter()
        .find(|addr| addr.is_tcp() && !addr.is_webrtc())
        .or_else(|| {
            paths
                .iter()
                .find(|addr| addr.is_quic_v1() && !addr.is_webrtc() && !addr.is_webtransport())
        });
    let webtransport = paths.iter().find(|addr| addr.is_webtransport());
    let browser = paths.iter().find(|addr| addr.is_webrtc());

    // 三类都没命中说明这一类只有别的传输，留一条免得整类地址丢空。
    let picked = if native.is_none() && webtransport.is_none() && browser.is_none() {
        [paths.first(), None, None]
    } else {
        [native, webtransport, browser]
    };
    for addr in picked.into_iter().flatten() {
        if !selected.contains(addr) {
            selected.push(addr.clone());
        }
    }
}

/// 邀请二维码的模块数上限（含两侧各 4 模块的 quiet zone）。
///
/// **这是密度上限，不是容量上限。** 容量从来不是约束（ECL::M 下 2079 字节才到顶）；
/// 真正会先出事的是 px/模块 —— 三端最小的码面是 **196px**（移动端白卡内沿
/// `220 - 2×12`、Web 端 `QR_SIZE`；桌面 260px 更宽松），跌破 2px/模块摄像头就读不出来。
/// 196 / 2 = 98。
///
/// 这个数是从码面尺寸推出来的，不是品味 —— 要放宽它，得先把三端的码面画大。
///
/// ⚠️ **跨语言常量，没有任何门禁把三处钉在一起**，改任一处都不会红：
/// `docs/app/app/_components/invite-share.tsx` 的 `QR_SIZE`、
/// `mobile/src/components/pairing/invite-qr.tsx` 的 `size = 220`（减去 12 的两侧内边距），
/// 与这里。两个 UI 文件的注释里都写着这条对应关系 —— 有人把码面缩小以适配窄断点时，
/// 这个预算就静默失真了，而症状只有真机扫不出来。
///
/// # 已知代价：桌面按最小码面受委屈（2026-08-12 评估，暂不改）
///
/// 真正决定能否扫出来的是**生成这张码那一端**的码面，而桌面是 260px（≈130 模块的余量）。
/// 按 196px 一刀切的后果是：满配桌面（覆盖网 + 私网 + 公网 + circuit 同时在场）117 模块
/// 被判超标，WebTransport 整个被裁掉。
///
/// 没有跟着参数化，是权衡的结果：
/// - **代价有限** —— 丢的是「首次拨号快一点」，不是可达性（见下面丢弃顺序第 1 条）；
///   而且最常见的家用档位（私网 + circuit）本来就放得下，测试里那一档断言 WebTransport
///   必须留下。真正吃亏的是同时挂着覆盖网又有公网监听地址的那一小撮机器。
/// - **修法都不便宜** —— 把预算变成 `encode_invite` 的参数要动三端 IPC/FFI 签名
///   （其中 uniffi 那条要重新生成移动端桥接）；而若让各端在 Rust 侧各存一份码面常量，
///   上面那条「跨语言常量没有门禁」的漂移面直接翻倍。
///
/// 要做的话，正确形态是**让 UI 把自己的码面尺寸传进来**（那是它唯一知道、且本来就
/// 定义在那里的数），core 只留 `MIN_PX_PER_MODULE`，跨语言副本随之归零。
const INVITE_QR_MAX_MODULES: usize = 98;

/// 编码邀请，并在码面过密时**逐条回收地址**直到扫得动。
///
/// # 为什么必须有这一步
///
/// 地址是「每桶挑几条 × 桶数」，而**桶数没有上界**：一台同时有 CGNAT 覆盖网、局域网、
/// 公网直连的机器就是 3 个桶。实测（`invite_stays_scannable_at_every_scale`）加 WebTransport
/// 之前，满配桌面的码面已经是 97 模块 —— 距上限只剩 1 格。也就是说这里在改动之前就没有
/// 余量了，**再往邀请里加任何东西都会静默越线**：QR 照样生成、链接照样能用，只有真机扫码
/// 那一下失败，而那是最难归因的失败形态。
///
/// # 丢弃顺序 = 价值的反序
///
/// 1. **WebTransport** —— 纯增益。丢掉后浏览器仍能靠同桶的 webrtc-direct 拨通；配对一旦
///    成功，identify 会把完整监听地址交回来，后续传输照样走 WebTransport。代价只是首次
///    拨号慢一点，不是连不上。同类里**私网（`lan`）那条留到最后**，其余从后往前丢 ——
///    那 4.5 倍只在真正同网时兑现，而同网指的是 RFC1918，不是 100.64/10 覆盖网。
/// 2. **其余直连地址**，同样从后往前。
/// 3. **circuit 一条都不丢**，**最后一条地址也一条都不丢**（哪怕它不是 circuit）。
///    前者是因为跨网时 circuit 是唯一可达路径，而扫码方在哪个网络、生成邀请的这一端
///    并不知道；后者是因为零地址的邀请**一切正常只是拨不动**，是这里最坏的输出形态。
///
/// 于是这个函数有一个**下界而非终点**：它保证「裁到扫得动」是尽力而为，不保证一定做到。
/// 真裁不下去时返回的是一条密度超标但语义完好的邀请 —— 用户还能改用粘贴链接。
///
/// 逐次重编码而不是估算字节：判据就是最终码面本身，零推导误差。地址至多十来条、
/// 签名是 ed25519，生成邀请又是低频动作，这点开销无关紧要。
fn fit_invite_to_scannable(mut invite: PairInvite, secret: &swarmdrop_net::SecretKey) -> String {
    loop {
        let encoded = invite.encode(secret);
        let scannable = swarmdrop_invite::invite_qr_matrix(&encoded)
            // 编不出码（真的超了 QR 容量）与「码面太密」在这里是同一件事：都得再丢一条。
            .is_ok_and(|m| m.len() <= INVITE_QR_MAX_MODULES);
        if scannable || !drop_least_valuable_addr(&mut invite.inviter.addrs) {
            return encoded;
        }
    }
}

/// 丢掉一条最不值钱的地址提示，没有可丢的返回 `false`。丢弃顺序见
/// [`fit_invite_to_scannable`]。
fn drop_least_valuable_addr(addrs: &mut Vec<Addr>) -> bool {
    // **最后一条谁都不许动。** 零地址的邀请编得出、扫得动、也复制得走，唯独没有任何
    // 东西可拨 —— 对方拿到它只会静默连不上，两端都不会报错。宁可给一条密度超标的码
    // （用户还能改用粘贴链接），也不给一条形式完好、语义为空的邀请。
    //
    // ⚠️ 这条闸不能只靠下面那个 `!is_circuit()` 兜：`LocalOnly` 邀请**根本没有 circuit
    // 地址**（`select_invite_addrs` 只放私网那一桶），设备名再长一点就会一路裁到空。
    if addrs.len() <= 1 {
        return false;
    }

    // `rposition` = 从后往前。桶序是 shared → lan → public，所以「从后往前」等于
    // 「公网 → 局域网 → 覆盖网」——**而这恰恰把最该留的那条排在了中间**：真正同网的是
    // `lan`（RFC1918），不是 `shared`（100.64/10 覆盖网 / mesh VPN）。而 WebTransport
    // 那 4.5 倍只在同网兑现，所以 lan 那条要留到最后。
    //
    // 这个次序在最常见的密度压力源上就会体现：笔记本挂着 Tailscale 时 shared 桶才有东西，
    // 天真的「从后往前」会先扔掉 lan 的 WebTransport、把覆盖网那条留下来。
    // ⚠️ **三条规则都必须排除 circuit**，不能只让最后一条兜。circuit 基址是从地址簿里
    // 任取一条带传输段的地址拼出来的（`circuit_base` 刻意不做传输白名单），而 bootstrap
    // 就监听 WebTransport —— 于是 `<relay>/…/webtransport/…/p2p-circuit` 是**很常见**的
    // 形态，它同时满足 `is_webtransport()` 与 `!is_private_lan()`，正好命中第一条规则，
    // 结果是「circuit 一条都不丢」这条不变量反过来变成「circuit 最先丢」：邀请编得出、
    // 扫得动，跨网时却零可达路径。
    let victim = addrs
        .iter()
        .rposition(|a| a.is_webtransport() && !a.is_circuit() && !a.is_private_lan())
        .or_else(|| {
            addrs
                .iter()
                .rposition(|a| a.is_webtransport() && !a.is_circuit())
        })
        .or_else(|| addrs.iter().rposition(|a| !a.is_circuit()));
    match victim {
        Some(i) => {
            addrs.remove(i);
            true
        }
        // 只剩 circuit：不丢。跨网时它是唯一可达路径，理由同上面那条下界。
        None => false,
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

/// 配对域需要的宿主端口。
///
/// 四项**只服务配对域**：[`NetManager`](crate::network::NetManager) 一项都不用，只是把
/// 它整体转交下来。收成一个类型正是为了让这件事在签名上就看得见——散成四个位置参数时，
/// `NetManager::new` 看起来像是自己也要用它们。
///
/// 刻意**不含** `DeviceConfig`：设备名的落盘必须排在推网络之前，那个顺序住在
/// [`rename_device`](crate::device_name::rename_device)。把它递到这一层，等于为「在配对
/// 域里顺手存一下名字」开一条绕过该顺序的路——用户会看到「改成功了」、重启却变回旧名字。
pub struct PairingPorts {
    /// 入站配对请求到达时发 [`CoreEvent::PairingRequestReceived`](crate::host::CoreEvent)。
    pub event_bus: Arc<dyn EventBus>,
    /// 入站请求的系统通知。`None` = 该端没有这个概念（移动端与浏览器）。
    pub notifier: Option<Arc<dyn Notifier>>,
    /// 邀请注册表的落盘端口（native = SQL / wasm = IndexedDB）；传
    /// [`NoopInviteStore`](swarmdrop_invite::NoopInviteStore) 退回「重启丢邀请」的旧语义。
    /// **构造后需调用 [`PairingManager::load_invites`]** 把已落盘的邀请读回内存表，
    /// 否则重启后它们等同不存在。
    pub invite_store: Arc<dyn InviteStore>,
    /// 已配对设备列表的持久化端口（桌面 = `paired-devices.json` / 移动 = 系统安全存储 /
    /// wasm = IndexedDB），[`PairingManager::unpair`] 靠它把解除配对做成原子操作。
    pub paired_store: Arc<dyn PairedDeviceStore>,
}

impl PairingManager {
    /// `os_info` 是本机设备信息，由组合根注入——**不要在这里 `OsInfo::default()`**，
    /// 那正是「用户设的名字进不了配对请求、也进不了邀请串」这条 bug 的成因。
    pub fn new(
        endpoint: Endpoint,
        os_info: OsInfo,
        paired_devices: Arc<DashMap<NodeId, PairedDeviceInfo>>,
        devices: Arc<DeviceManager>,
        ports: PairingPorts,
    ) -> Self {
        let PairingPorts {
            event_bus,
            notifier,
            invite_store,
            paired_store,
        } = ports;
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
        //
        // ⚠️ 顺序是「先登记后裁剪」，成立的依据是 `register` 读的三样
        //（`capability` / `expires_at` / `inviter.id`）**都不在裁剪范围内** —— 裁的只有
        // `inviter.addrs`。反过来写同样对，但那会让「注册表里已有它」的时刻更晚。
        //
        // 往这后面再加别的改动前，回头核对一遍这个交集：动了 `inviter.id` 就会让已落盘的
        // 记录与发出去的串指向不同身份，而两边都不会报错。
        self.invite_registry.register(&invite, now).await;
        fit_invite_to_scannable(invite, secret)
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
        // 回给了对端，而落盘是一次真实 I/O（桌面文件 / Web IndexedDB 往返 / 移动平台
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
            PairingPorts {
                event_bus,
                notifier: None,
                invite_store: Arc::new(NoopInviteStore),
                paired_store,
            },
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
            PairingPorts {
                event_bus: Arc::new(host.clone()),
                notifier: None,
                invite_store: Arc::new(NoopInviteStore),
                paired_store: Arc::new(host),
            },
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

    /// 一条 WebTransport 监听地址（两个 certhash = 轮换期两张证书都在场，真机形态）。
    fn webtransport(ip: &str, port: u16) -> String {
        format!(
            "/ip4/{ip}/udp/{port}/quic-v1/webtransport\
             /certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g\
             /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
        )
    }

    #[test]
    fn auto_invite_keeps_bounded_direct_and_relay_paths() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let circuit_tcp = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit");
        let circuit_quic = format!("/ip4/203.0.113.9/udp/4001/quic-v1/p2p/{relay_id}/p2p-circuit");
        let circuit_webrtc = format!("/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit/webrtc");
        let shared_wt = webtransport("100.100.200.77", 4004);
        let lan_wt = webtransport("192.168.1.10", 4004);
        let public_wt = webtransport("198.51.100.10", 4004);
        let selected = select_invite_addrs(
            vec![
                addr("/ip4/127.0.0.1/tcp/4001"),
                addr("/ip4/100.100.200.77/tcp/4001"),
                addr("/ip4/100.100.200.77/udp/4001/quic-v1"),
                addr(&shared_wt),
                addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr("/ip4/192.168.1.10/udp/4001/quic-v1"),
                addr(&lan_wt),
                addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
                addr("/ip4/198.51.100.10/tcp/4001"),
                addr("/ip4/198.51.100.10/udp/4001/quic-v1"),
                addr(&public_wt),
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
            addr(&shared_wt),
            addr("/ip4/100.100.200.77/udp/4002/webrtc-direct"),
            addr("/ip4/192.168.1.10/tcp/4001"),
            addr(&lan_wt),
            addr("/ip4/192.168.1.11/udp/4002/webrtc-direct"),
            addr("/ip4/198.51.100.10/tcp/4001"),
            addr(&public_wt),
            addr("/ip4/198.51.100.10/udp/4002/webrtc-direct"),
            addr(&circuit_tcp),
            addr(&circuit_webrtc),
        ];

        assert_eq!(selected, expected);
    }

    /// WebTransport 地址**不得占用 native 那一格**。
    ///
    /// `is_quic_v1()` 对它为真（地址里确有 `/quic-v1` 段），所以一张只有
    /// 「QUIC + WebTransport」的网卡上，漏掉 `!is_webtransport()` 排除条件的实现会让
    /// native 与 webtransport 两格挑到同一条地址 —— 去重之后整个类别只剩一条，
    /// 真正的裸 QUIC 路径被静默丢掉。`assert_eq!` 比对整条清单是抓不到这个的
    /// （上面那条测试里 TCP 总是先命中），所以单列一条。
    #[test]
    fn webtransport_never_takes_the_native_slot() {
        let wt = webtransport("192.168.1.10", 4004);
        let selected = select_invite_addrs(
            vec![addr(&wt), addr("/ip4/192.168.1.10/udp/4001/quic-v1")],
            TransportPolicy::LocalOnly,
        );

        assert_eq!(
            selected,
            vec![addr("/ip4/192.168.1.10/udp/4001/quic-v1"), addr(&wt)],
            "裸 QUIC 与 WebTransport 必须各占一格，且 native 排在前"
        );
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

    /// 四种真实网络配置下的 QR 密度回归钉。
    ///
    /// 判据是**扫得动**，不是「编得出」—— 容量从来不是约束（ECL::M 下 2079 字节才到顶），
    /// 先出事的永远是 px/模块（见 [`INVITE_QR_MAX_MODULES`]）。
    ///
    /// ⚠️ **这条测试是加 WebTransport 时补的，而它一补上就发现改动前也已经卡线**：
    /// 满配桌面（CGNAT 覆盖网 + 局域网 + 公网直连 + 中继）当时是 97 模块，距上限 98
    /// 只剩一格。所以裁剪不是给 WebTransport 擦屁股的补丁，是这个函数一直缺的那道闸。
    ///
    /// 实测模块数（含 quiet zone，`INJECTED_NAME` 作设备名，上限 98）：
    ///
    /// | 配置 | 不带 WebTransport | 带上但不裁 | 现在（带上 + 裁） |
    /// |---|---|---|---|
    /// | 家用 lan + circuit | 85 | 93 | **93**（5 条，一条没裁） |
    /// | 公网 lan + public + circuit | 89 | 105 | **97**（8 → 7，保住 WebTransport） |
    /// | CGNAT shared + lan + circuit | 89 | 105 | **97**（8 → 7，保住 WebTransport） |
    /// | 满配 四类齐全 | 97 | 117 | **97**（11 → 8，WebTransport 全裁） |
    ///
    /// 也就是说三档常见配置都放得下它，只有四类地址空间同时在场的极端机器才回退到
    /// 改动前的形态 —— 那正是 [`fit_invite_to_scannable`] 里丢弃顺序想要的结果。
    #[test]
    fn invite_stays_scannable_at_every_scale() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        // 一张网卡监听全部四种传输 —— 真机上每类地址空间都长这样。
        // （`//` 不是 `///`：doc comment 挂不到 `let` 上，rustc 会报 `unused_doc_comments`。）
        let nic = |ip: &str| {
            vec![
                addr(&format!("/ip4/{ip}/tcp/4001")),
                addr(&format!("/ip4/{ip}/udp/4001/quic-v1")),
                addr(&webtransport(ip, 4004)),
                addr(&format!(
                    "/ip4/{ip}/udp/4002/webrtc-direct\
                     /certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA"
                )),
            ]
        };
        // ⚠️ **第三条必须是 WebTransport 基址的 circuit。** circuit 基址取自地址簿里任一
        // 带传输段的地址（`circuit_base` 刻意不做白名单），而 bootstrap 本身就监听
        // WebTransport —— 这个形态在真机上很常见，且它同时满足 `is_webtransport()` 与
        // `!is_private_lan()`。裁剪规则若不显式排除 circuit，它会被**最先**丢掉，
        // 而只用 TCP circuit 的测试永远抓不到（那是这条回归躲过第一版审查的原因）。
        let circuits = vec![
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit"
            )),
            addr(&format!(
                "/ip4/203.0.113.9/tcp/4001/p2p/{relay_id}/p2p-circuit/webrtc"
            )),
            addr(&format!(
                "{}/p2p/{relay_id}/p2p-circuit",
                webtransport("203.0.113.9", 4004)
            )),
        ];
        // 由少到多。前两档必须**保住** WebTransport（那正是补它的意义），
        // 后两档允许裁，但一条 circuit 都不许少。
        let scenarios: Vec<(&str, Vec<Addr>, bool)> = vec![
            (
                "家用 lan+circuit",
                [nic("192.168.1.10"), circuits.clone()].concat(),
                true,
            ),
            (
                "公网 lan+public+circuit",
                [nic("192.168.1.10"), nic("198.51.100.10"), circuits.clone()].concat(),
                false,
            ),
            (
                "CGNAT shared+lan+circuit",
                [nic("100.100.200.77"), nic("192.168.1.10"), circuits.clone()].concat(),
                false,
            ),
            (
                "满配 shared+lan+public+circuit",
                [
                    nic("100.100.200.77"),
                    nic("192.168.1.10"),
                    nic("198.51.100.10"),
                    circuits.clone(),
                ]
                .concat(),
                false,
            ),
        ];

        for (name, dialable, keeps_webtransport) in scenarios {
            let selected = select_invite_addrs(dialable, TransportPolicy::Auto);
            let before = selected.len();
            let circuits_before = selected.iter().filter(|a| a.is_circuit()).count();
            let secret = SecretKey::generate();
            let invite = swarmdrop_invite::PairInvite {
                // 固定 capability：`generate` 那份是随机的，而 base32 payload 里数字连段的
                // 长短会改变最优分段的取舍，码面因此浮动一档（qr.rs 的 `fixed_invite` 记着
                // 同一件事）。回归钉要可复现。
                capability: [
                    0x3f, 0x1c, 0xa7, 0x02, 0x9b, 0x64, 0xd8, 0x51, 0x0e, 0xf3, 0x27, 0xbc, 0x45,
                    0x8a, 0x16, 0xd0,
                ],
                inviter: swarmdrop_net::NodeAddr::with_addrs(secret.node_id(), selected),
                expires_at: 1_700_086_400,
                transport_policy: TransportPolicy::Auto,
                display_name: INJECTED_NAME.into(),
                display_platform: "macos".into(),
            };
            let encoded = fit_invite_to_scannable(invite, &secret);
            let modules = swarmdrop_invite::invite_qr_matrix(&encoded)
                .expect("裁剪后必须仍编得出码")
                .len();
            assert!(
                modules <= INVITE_QR_MAX_MODULES,
                "{name}：码面 {modules} 模块超过 196px 下可扫的 {INVITE_QR_MAX_MODULES}\
                 （邀请串 {} 字符）",
                encoded.len()
            );

            // 裁剪的是地址，解码回来才能看清它到底留下了什么。
            let decoded = swarmdrop_invite::PairInvite::decode(&encoded).expect("自签邀请可解码");
            // **数量断言，不是 `any`。** `any(is_circuit())` 只要还剩一条就绿，于是
            // 「裁掉了 WebTransport 基址那条 circuit」这类回归它一个都抓不到 —— 而
            // 「一条都不许裁」本来就是按条数说的。
            let circuits_after = decoded
                .inviter
                .addrs
                .iter()
                .filter(|a| a.is_circuit())
                .count();
            assert_eq!(
                circuits_after, circuits_before,
                "{name}：circuit 是跨网唯一可达路径，一条都不许裁\
                 （{circuits_before} → {circuits_after}）"
            );
            if keeps_webtransport {
                assert!(
                    decoded.inviter.addrs.iter().any(|a| a.is_webtransport()),
                    "{name}：这一档放得下 WebTransport，裁掉它等于白加（留下 {} 条地址）",
                    decoded.inviter.addrs.len()
                );
            } else {
                // 唯一放不下的那一档：断言闸**确实动了手**。少了这条，一个恒等于「不裁」
                // 的实现也能让上面的码面断言通过 —— 因为它本来就压着上限。
                assert!(
                    decoded.inviter.addrs.len() < before,
                    "{name}：码面本该超标，裁剪却一条没动（{before} 条原样留下）"
                );
            }
        }
    }

    /// **零地址的邀请是最坏的输出形态**，裁剪必须留住最后一条。
    ///
    /// `LocalOnly` 邀请根本没有 circuit 地址（只放私网那一桶），所以「circuit 不丢」这条
    /// 规则对它一点保护都没有 —— 少了下界，设备名长一点就会一路裁到空，而那样的邀请
    /// **编得出、扫得动、复制得走，唯独没有任何东西可拨**，两端都不会报错。
    #[test]
    fn trimming_never_empties_the_address_hints() {
        let mut addrs = vec![
            addr("/ip4/192.168.1.10/tcp/4001"),
            addr(&webtransport("192.168.1.10", 4004)),
        ];
        assert!(drop_least_valuable_addr(&mut addrs), "两条时该丢一条");
        assert_eq!(addrs.len(), 1);
        assert!(
            !drop_least_valuable_addr(&mut addrs),
            "只剩一条时必须停手，哪怕它不是 circuit"
        );
        assert_eq!(addrs.len(), 1, "最后一条不许丢");
    }

    /// **WebTransport 基址的 circuit 不是「一条 WebTransport」，它是 circuit。**
    ///
    /// circuit 基址取自地址簿里任一带传输段的地址（`circuit_base` 刻意不做传输白名单），
    /// 而 bootstrap 本身就监听 WebTransport —— 所以
    /// `<relay>/…/webtransport/…/p2p-circuit` 是真机上很常见的形态。它同时满足
    /// `is_webtransport()` 与 `!is_private_lan()`，若裁剪的前两条规则不显式排除 circuit，
    /// 它会被**最先**丢掉：「circuit 一条都不丢」这条不变量反过来变成「circuit 最先丢」，
    /// 而邀请照样编得出、扫得动，只是跨网时零可达路径。
    ///
    /// 走整条编码链路的 `invite_stays_scannable_at_every_scale` 抓不到它——那里还有别的
    /// circuit 顶着，条数断言才刚补上。这一条直接打在裁剪函数上，形态最小、失败最明确。
    #[test]
    fn trimming_never_sacrifices_a_webtransport_based_circuit() {
        let relay_id = "12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp";
        let mut addrs = vec![
            // 公网直连的 WebTransport：该它先走。
            addr(&webtransport("198.51.100.10", 4004)),
            // 经中继的 WebTransport circuit：跨网唯一可达路径，一条都不许动。
            addr(&format!(
                "{}/p2p/{relay_id}/p2p-circuit",
                webtransport("203.0.113.9", 4004)
            )),
        ];

        assert!(drop_least_valuable_addr(&mut addrs), "两条时该丢一条");
        assert_eq!(addrs.len(), 1);
        assert!(
            addrs[0].is_circuit(),
            "留下的必须是 circuit —— 丢掉它等于把跨网唯一可达路径先丢了"
        );
    }

    /// 同网那条 WebTransport 留到最后 —— 它那 4.5 倍只在 RFC1918 上兑现。
    ///
    /// 这条钉的是**次序**而不是「丢了几条」：桶序是 shared → lan → public，天真的
    /// 「从后往前」会得到 public → **lan** → shared，把最该留的那条排在中间。
    /// 而挂着 Tailscale 的笔记本（shared 桶有东西）正是密度压力最常见的来源。
    #[test]
    fn webtransport_in_the_lan_bucket_is_dropped_last() {
        let shared_wt = webtransport("100.100.200.77", 4004);
        let lan_wt = webtransport("192.168.1.10", 4004);
        let public_wt = webtransport("198.51.100.10", 4004);
        let mut addrs = select_invite_addrs(
            vec![
                addr("/ip4/100.100.200.77/tcp/4001"),
                addr(&shared_wt),
                addr("/ip4/192.168.1.10/tcp/4001"),
                addr(&lan_wt),
                addr("/ip4/198.51.100.10/tcp/4001"),
                addr(&public_wt),
            ],
            TransportPolicy::Auto,
        );

        let mut dropped = Vec::new();
        for _ in 0..3 {
            let before = addrs.clone();
            assert!(drop_least_valuable_addr(&mut addrs));
            dropped.push(
                before
                    .into_iter()
                    .find(|a| !addrs.contains(a))
                    .expect("每轮恰好少一条"),
            );
        }

        assert_eq!(
            dropped,
            vec![addr(&public_wt), addr(&shared_wt), addr(&lan_wt)],
            "WebTransport 的丢弃次序必须是 公网 → 覆盖网 → 局域网"
        );
    }

    /// 兜底分支：某一类只剩本函数不认识的传输时，仍要留一条，不能把整类丢空。
    /// 当前 transport 栈（TCP / QUIC / WebTransport / WebRTC）走不到这里，
    /// 它是新增传输时的安全网。
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
