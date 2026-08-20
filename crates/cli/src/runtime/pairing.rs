//! 入站配对请求的接纳。
//!
//! **谁在等，谁确认。** 配对是在建立信任边界，所以它的判据不能是「对端是谁」——那正是
//! 待判定的东西。本层把判据落在两件事上：
//!
//! 1. **有没有凭证**。不带邀请的局域网直连请求就地拒掉，连问都不问：它唯一的授权依据
//!    是「在本机 mDNS 多播域内」，而那不构成授权。推论：两个 CLI 之间不能用局域网直连
//!    配对，必须走邀请。
//! 2. **此刻有没有人在等一次配对**。带凭证的请求交给人确认；**没有人在等就拒绝**。
//!    「没人在等」意味着这台机器上没有任何人正期待一次配对，此刻到来的请求要么是抢配对，
//!    要么是重放，接受它没有正当理由。
//!
//! ⚠️ **邀请本身不构成「可以直接接受」。** 一张邀请只要泄露一次（截图、投屏、日志、
//! 抢先扫码）就能被别人用掉，而它是一次性的——被抢走的那次配对会**消耗掉凭证**，
//! 真正的设备再来就用不了了。所以默认必须由人看着对端信息点头。自动接受只在用户
//! 显式要求时发生（`--auto-accept`），那是把风险交换成无人值守能力的场景。
//!
//! 拒绝**不消费**凭证（core 只在接受时才 CAS 消费），所以被抢配对拒掉之后，
//! 同一张邀请对真正的设备仍然有效。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use swarmdrop_core::device::ConnectionType;
use swarmdrop_core::device_manager::DeviceFilter;
use swarmdrop_core::host::CoreEvent;
use swarmdrop_core::pairing::{PairedDeviceCommit, persisted_or_absent};
use swarmdrop_core::protocol::{PairingMethod, PairingRefuseReason, PairingResponse};
use swarmdrop_net::NodeId;
use tokio::sync::{Mutex, mpsc};

use super::boot::RunningNode;

/// 一次长轮询最多挂多久。
///
/// 它是**僵尸等待者的回收周期**：`pair` 客户端被 Ctrl-C 掉之后，服务端这侧那个取请求的
/// 任务还会挂着，在它超时之前配对窗口都算「开着」。取短一点是为了压小这段窗口——
/// 代价只是每隔这么久重连一次本地套接字。
///
/// 那段窗口内到达的请求会被交给一个已经没人接的客户端，于是没有任何人应答它。
/// **这不是安全缺口**：兜底是核心自己的入站超时（170s），到期一律婉拒——
/// 后果是对端多等一会儿才被拒，而不是被放行。
const POLL_TIMEOUT: Duration = Duration::from_secs(15);

/// 这个请求要不要拿去问人。
///
/// `false` = 不带凭证，就地拒绝，不惊动任何人——它唯一的授权依据是「在本机 mDNS
/// 多播域内」，那不构成授权，为它弹一次确认只是白白多一个可被利用的骚扰面。
///
/// 穷尽 match 而非 `matches!`：新增配对方法时这里会编译失败，而不是静默落进某一侧。
pub fn needs_confirmation(method: &PairingMethod) -> bool {
    match method {
        PairingMethod::Invite { .. } => true,
        PairingMethod::Direct => false,
    }
}

/// 一次待确认的入站配对请求，含用户判断所需的全部信息。
///
/// **完整节点标识必须给全，不能截断**：设备名是对端自己报的、可以随便填，而节点标识是
/// 公钥的哈希，与传输层握手校验的是同一个身份。用户要靠它和对方口头核对——「被抢配对」
/// 这件事，能拦住它的就是这一串字符。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub pending_id: u64,
    pub peer_id: String,
    pub device: String,
    pub os: String,
    pub arch: String,
    /// 这个请求是从哪条链路来的。
    ///
    /// **它比设备名更接近用户能验证的事实**：期待身边同事配对时看到「经中继转发」，
    /// 就该停下来问一句。`None` = 内核此刻还没给出链路判定（刚建连的一瞬）。
    pub connection: Option<ConnectionType>,
}

/// 应答一次入站配对请求，返回这次配对的结局。
///
/// 应答失败时返回 `None`——调用方据此知道「这次请求没被处理」（对端已断开或核心那侧
/// 已经等超时），而不是误以为拒绝了。**拒绝是 `Some(结局.accepted == false)`**，
/// 与「没能应答」是两件事。
///
/// **本层不出面向用户的文案**（见 [`super`] 的约束）：措辞由 [`crate::render`] 决定，
/// 这里只留 tracing 供服务单元的日志排查。
pub async fn respond(node: &RunningNode, pending_id: u64, accept: bool) -> Option<PairOutcome> {
    let response = if accept {
        PairingResponse::Success
    } else {
        PairingResponse::Refused {
            reason: PairingRefuseReason::UserRejected,
        }
    };

    match node
        .manager
        .pairing()
        // clone：`response` 随后还要用来构造 `PairOutcome`，而核心按值取走它。
        .respond_pairing_request(pending_id, response.clone())
        .await
    {
        Ok(commit) => Some(PairOutcome::new(&response, &commit)),
        Err(err) => {
            tracing::warn!(pending_id, "应答入站配对请求失败: {err}");
            None
        }
    }
}

/// 入站配对请求流：订阅、过滤、就地拒掉不带凭证的那些。
///
/// **这套机制只写一遍**。两个消费者的差别只在「谁来确认」——常驻节点把请求转交给
/// `pair` 客户端，临时节点自己问。各写一份的话，[`crate::cmd`] 就得直接认识 `CoreEvent`
/// 与事件通道，而那层明确不含网络细节。
pub struct InboundPairings {
    events: mpsc::UnboundedReceiver<CoreEvent>,
}

impl InboundPairings {
    pub fn subscribe(node: &RunningNode) -> Self {
        Self {
            events: node.events.subscribe(),
        }
    }

    /// 等到下一个**需要人确认**的入站配对请求。
    ///
    /// 不带凭证的直连请求在这里就地拒掉，不返回给调用方——它不需要惊动任何人，
    /// 而每一次「惊动」都是一次可以被利用的骚扰面。
    ///
    /// `None` 表示事件通道已断开——只发生在节点关停时。
    pub async fn next(&mut self, node: &RunningNode) -> Option<PairingRequest> {
        while let Some(event) = self.events.recv().await {
            let CoreEvent::PairingRequestReceived {
                peer_id,
                pending_id,
                request,
            } = event
            else {
                continue;
            };

            let who = request.os_info.display_name();
            if !needs_confirmation(&request.method) {
                tracing::warn!(who, "已拒绝无凭证的局域网直连配对请求");
                respond(node, pending_id, false).await;
                continue;
            }

            return Some(PairingRequest {
                pending_id,
                peer_id: peer_id.to_string(),
                device: who,
                os: request.os_info.os.clone(),
                arch: request.os_info.arch.clone(),
                connection: connection_of(node, &peer_id),
            });
        }
        None
    }
}

/// 查这个 peer 此刻是从哪条链路连进来的。
///
/// 过滤器用 `All` 而不是 `Paired`——**要找的正是一台还没配对的设备**。
fn connection_of(node: &RunningNode, peer_id: &NodeId) -> Option<ConnectionType> {
    node.manager
        .devices()
        .get_devices(DeviceFilter::All)
        .into_iter()
        .find(|device| &device.peer_id == peer_id)
        .and_then(|device| device.connection)
}

/// 常驻节点的确认台。
///
/// 常驻节点自己问不了人——它多半跑在后台或服务单元里，没有 stdin。所以它把待确认的
/// 请求**转交给正在等待的 `pair` 客户端**，由那个终端前的人来决定。
///
/// 于是「配对窗口」有了一个明确的开合判据：**只有你执行 `swarmdrop invite create` 时它才开着**。
/// 其余时间到来的配对请求一律被拒——这正是想要的：没有人在等配对的时候，任何配对请求
/// 都是意外的。
pub struct ConfirmationDesk {
    /// 容量 1。同一时刻只放一个待确认请求——否则抢配对的一方可以刷请求把用户淹没，
    /// 逼他在一串几乎相同的提示里误点一次「是」。
    tx: mpsc::Sender<PairingRequest>,
    rx: Mutex<mpsc::Receiver<PairingRequest>>,
    /// 此刻有几个客户端阻塞在取请求上。**它就是「配对窗口开着吗」的答案**。
    waiting: AtomicUsize,
}

impl Default for ConfirmationDesk {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self {
            tx,
            rx: Mutex::new(rx),
            waiting: AtomicUsize::new(0),
        }
    }
}

impl ConfirmationDesk {
    /// 事件侧：把请求交给等待中的客户端。
    ///
    /// 返回 `false` 表示没人接（没有客户端在等，或已经有一个请求在等确认了），
    /// 调用方**必须立刻拒绝**那个请求——放着不管的话对端要卡满核心的入站超时。
    pub fn offer(&self, request: PairingRequest) -> bool {
        if self.waiting.load(Ordering::SeqCst) == 0 {
            return false;
        }
        self.tx.try_send(request).is_ok()
    }

    /// 客户端侧：取一个待确认的请求，最多等 [`POLL_TIMEOUT`]。
    ///
    /// `None` 表示本轮没有请求，调用方应立即再取一次——「有没有人在等」正是由这个
    /// 反复轮询表达的，停下来就等于关上了配对窗口。
    pub async fn take(&self) -> Option<PairingRequest> {
        let _open = WindowGuard::open(&self.waiting);
        let mut rx = self.rx.lock().await;
        tokio::time::timeout(POLL_TIMEOUT, rx.recv())
            .await
            .ok()
            .flatten()
    }
}

/// 配对窗口的开启凭据：活着的时候窗口就开着。
///
/// 用 RAII 而不是手动加减：取请求那段会因超时、连接断开、任务被取消而从多个位置退出，
/// 漏掉任何一处都会让窗口**永久**开着——那时它就不再是一道闸了。
struct WindowGuard<'a>(&'a AtomicUsize);

impl<'a> WindowGuard<'a> {
    fn open(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for WindowGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 起一个后台任务，把入站配对请求交给确认台；没人接就拒绝。
///
/// `auto_accept` 为真时跳过确认台一律接受——那是 `--auto-accept` 的无人值守形态，
/// 用户已经显式表达过「我知道风险，别问我」。
pub fn spawn_desk_service(node: Arc<RunningNode>, desk: Arc<ConfirmationDesk>, auto_accept: bool) {
    let mut inbound = InboundPairings::subscribe(&node);

    tokio::spawn(async move {
        while let Some(request) = inbound.next(&node).await {
            let who = request.device.clone();
            let pending_id = request.pending_id;

            if auto_accept {
                respond(&node, pending_id, true).await;
                tracing::info!(who, "已自动接受入站邀请配对（--auto-accept）");
                continue;
            }

            if desk.offer(request) {
                tracing::info!(who, "入站配对请求已转交给等待中的客户端确认");
            } else {
                // 立刻拒绝，不留给核心去超时：对端等三分钟才收到「已拒绝」，
                // 与本机此刻就能给出的答案没有任何差别，只是把代价转嫁过去。
                respond(&node, pending_id, false).await;
                tracing::warn!(
                    who,
                    "已拒绝入站配对请求：此刻没有人在等待配对（需要在本机执行 swarmdrop invite create）"
                );
            }
        }
    });
}

/// 一次「以邀请配对」的结局。
///
/// 本进程自持节点与经本地通道复用常驻节点是两条独立代码路径，**共用这一个形状**——
/// 否则「对端拒绝」会在两条路径上长出两种表达，而只有其中一条会被测到。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairOutcome {
    /// 对端是否接受。**这一位不能省**：RPC 成功只说明问答走完了，答案完全可能是「拒绝」。
    pub accepted: bool,
    /// 对端设备名；被拒时为 `None`——那时根本没有设备可记。
    pub device: Option<String>,
    /// `false` = 本次运行内可用，但重启后会丢。
    pub persisted: bool,
}

impl PairOutcome {
    pub fn new(response: &PairingResponse, commit: &Option<PairedDeviceCommit>) -> Self {
        Self {
            accepted: matches!(response, PairingResponse::Success),
            device: commit.as_ref().map(|c| c.device.os_info.display_name()),
            // 没配成时这里是 `true`，见 core 的 `persisted_or_absent`：那时没有该落盘的
            // 东西，报 `false` 会让用户看到一句无从解释的「配对成功但没保存」。
            persisted: persisted_or_absent(commit.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pending_id: u64) -> PairingRequest {
        PairingRequest {
            pending_id,
            peer_id: "12D3KooWTest".into(),
            device: "某台设备".into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            connection: Some(ConnectionType::Lan),
        }
    }

    /// 邀请要人确认、直连直接拒——这两条就是接纳策略的全部。
    /// **邀请不是「可以直接接受」**：它会泄露、会被抢先用掉，而消费掉之后真正的设备
    /// 就配不上了。
    #[test]
    fn invite_needs_confirmation_direct_is_refused() {
        assert!(needs_confirmation(&PairingMethod::Invite {
            capability: [0; 16]
        }));
        assert!(!needs_confirmation(&PairingMethod::Direct));
    }

    /// **没有人在等的时候，确认台不收请求**——调用方据此立刻拒绝。
    /// 这是「配对窗口只在你执行 pair 时才开」这条性质的落点。
    #[tokio::test]
    async fn desk_refuses_when_nobody_waits() {
        let desk = ConfirmationDesk::default();
        assert!(!desk.offer(sample(1)));
    }

    /// 有人在等时，请求应当被送到那个人手上。
    #[tokio::test]
    async fn desk_hands_request_to_a_waiter() {
        let desk = Arc::new(ConfirmationDesk::default());

        let taking = {
            let desk = desk.clone();
            tokio::spawn(async move { desk.take().await })
        };

        // 等 `take` 真正把窗口打开——否则测的是竞态而不是行为。
        while desk.waiting.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }

        assert!(desk.offer(sample(7)));
        let got = taking.await.expect("任务").expect("应当收到请求");
        assert_eq!(got.pending_id, 7);
    }

    /// 已经有一个请求在等确认时，第二个不进队——否则抢配对的一方可以刷请求
    /// 把用户淹没在一串几乎相同的提示里。
    #[tokio::test]
    async fn desk_holds_only_one_pending_request() {
        let desk = Arc::new(ConfirmationDesk::default());
        let _open = WindowGuard::open(&desk.waiting);

        assert!(desk.offer(sample(1)));
        assert!(!desk.offer(sample(2)));
    }

    /// 被拒绝的结局**不得**看起来像成功。
    #[test]
    fn refusal_is_not_success() {
        let refused = PairOutcome::new(
            &PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            },
            &None,
        );
        assert!(!refused.accepted);
        assert!(refused.device.is_none());
        // 没配成 ⇒ 不该报「没保存」，那句话对用户无从解释。
        assert!(refused.persisted);
    }

    /// 结局与待确认请求都要能经通道往返——两端是独立编译的代码路径，
    /// 形状对不上时表现是「配对流程卡住」而不是编译错误。
    #[test]
    fn wire_shapes_round_trip() {
        let value =
            serde_json::to_value(PairOutcome::new(&PairingResponse::Success, &None)).expect("编码");
        let back: PairOutcome = serde_json::from_value(value).expect("往返");
        assert!(back.accepted);

        let value = serde_json::to_value(sample(3)).expect("编码");
        let back: PairingRequest = serde_json::from_value(value).expect("往返");
        assert_eq!(back.pending_id, 3);
        assert_eq!(back.peer_id, "12D3KooWTest");
    }
}
