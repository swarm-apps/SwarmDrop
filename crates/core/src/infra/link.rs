//! 基础设施关系的读模型。
//!
//! # 为什么聚合根是「关系」不是「节点」
//!
//! 「引导节点」在领域里不是一个实体，而是**本机与某个远端之间的一段基础设施关系**。
//! [`InfraRoles`](swarmdrop_net::InfraRoles) 从内核第一天起就是两个正交 bool
//! （DHT 路由种子 / circuit 中继），本仓自建的那台恰好两者兼任，于是上层一路把它
//! 当成一个东西——直到同一个概念在五个写入点长出四种角色组合。
//!
//! 把节点换成关系之后，角色用 `Option<…>` 表达「是否承担」，分离部署时一台纯 kad
//! bootstrap 出来 `relay: None`，UI 不会再渲染一个永远「连接中」的假中继行。
//!
//! # 零存储
//!
//! [`build_infra_links`] 每次现场 join 三个权威源：候选表（意图）、
//! [`watch_conns`](swarmdrop_net::Endpoint::watch_conns)（连接事实）、
//! [`watch_relays`](swarmdrop_net::Endpoint::watch_relays)（reservation 事实）。
//! 观测值不落任何持久结构——「状态粘死」由结构保证，不靠调用纪律。这正是被删掉的
//! `CandidateHealth` 栽过的跟头：它把观测值写进候选表，而四条路径忘了回写，于是
//! 本机在公共 DHT 上发布失效的 relay hint，日志无痕。

use chrono::{DateTime, Utc};
use serde::Serialize;
use swarmdrop_net::{Addr, NodeId, RelayState};

use crate::network::SharedNetRefs;
use crate::network::candidates::{BootstrapCandidateSource, CandidateRoles, CandidateScope};

/// [`RelayState`] 的 IPC 投影。
///
/// `last_error` **原样保留内核下发的字符串**——这是三端唯一能说清「为什么连不上」
/// 的东西，排查时用户要贴的就是这一句，不翻译、不改写。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RelayLinkState {
    /// 正在拨号或等待 reservation 应答。
    Connecting,
    /// reservation 已建立，携带内核拼装的 circuit 可达地址（调用方不得自行拼接）。
    // 变体内字段要单独 rename：外层 `rename_all` 只改变体名，不下钻到字段
    // （本仓既有体例见 `FailureCode` / `WebError`）。漏了它 JS 侧拿到的是
    // `circuit_addr` 而三端其余字段都是 camelCase。
    #[serde(rename_all = "camelCase")]
    Active {
        #[cfg_attr(feature = "specta", specta(type = String))]
        circuit_addr: Addr,
    },
    /// 拨号失败或 reservation 失效，携带末次错误原文。**会自动重试**，文案不得给终态感。
    #[serde(rename_all = "camelCase")]
    Failed { last_error: String },
}

/// 一段基础设施关系当前不参与 relay 收敛的原因。
///
/// **说的是「设置」不是「故障」**：UI 必须中性色 + CTA 指向设置，不得升警示色、
/// 不得给「重试」。
///
/// 目前只有一个变体，且刻意不预留第二个（本仓既有规矩：不造到不了 UI 的判别码）：
/// - 没有 `NodeNotRunning`——节点运行态由 `NetworkStatus.status` 表达，而
///   `build_network_status` 里那个值恒为 `Running`，加进来就是一个永远为假的分支；
/// - 没有 `NotARelay`——今天**每一个**候选写入点都传 `CandidateRoles::kad_and_relay()`，
///   纯 kad 候选还不存在。等 `InfraSupervisor` 真按角色分档收敛、有了纯 kad 的写入方，
///   再连同它的产生者一起加回来。提前导出一个三端都渲染不出来的分支，是
///   `DESIGN.md:645-649`「永远为零的计数器比缺席更糟」在类型层的版本，而且它要过
///   specta / uniffi / wasm 三条 codegen；
/// - 没有任何基于 `DiscoveryMode` 的变体——那个轴当前零行为效果（全仓对该枚举无
///   `match`），基于它写新逻辑等于给一个待删的开关增加依赖。
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InfraExclusion {
    /// 公网范围候选，但用户关闭了公网可达性。
    PublicReachabilityDisabled,
}

/// 一段基础设施关系的完整读模型。
///
/// 字段分三段，**并置但不融合**：上半段只由意图路径写，下半段只由观测源现算。
/// 两者共享同一身份（`peer_id`），而用户唯一关心的正是两者的差——「我要它连上，
/// 它连上了吗」。拆成两个类型再让三端各自 join，只会把 join 做三遍。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InfraLink {
    // ── 意图侧｜权威源 = BootstrapCandidateManager ──
    #[cfg_attr(feature = "specta", specta(type = String))]
    pub peer_id: NodeId,
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub addrs: Vec<Addr>,
    pub sources: Vec<BootstrapCandidateSource>,
    pub roles: CandidateRoles,
    pub scope: CandidateScope,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    /// 用户能否在 UI 上移除这条：`sources` 里**含有** `HostConfigured` 即为 true。
    ///
    /// 纯自动来源（mDNS / Learned）**不给移除入口**：撤销会断开与该节点的全部连接
    /// （含在途传输），而局域网协助节点本身就可能是一台正在传文件的已配对设备；
    /// 何况它下次 identify 就会被原样登记回来——点了没反应，还把传输搞挂。
    ///
    /// ⚠️ 判据是「**含有**」不是「全是」。后者写过一版，是错的：`upsert` 对 `sources`
    /// 是**累加**（`candidates.rs`），而 `learn_candidate` 会给每个 identify 出
    /// `is_bootstrap_agent` 的对端补一条 `Learned`——本仓自建的那台正是 bootstrap agent，
    /// 于是它连上几秒后 `removable` 就翻假，移除按钮当场消失；用户自己加的中继更糟：
    /// 一旦连上就永久删不掉，而偏好清单还在每次启动回放它，`forgetInfraNode` 再无可达路径。
    /// 用户往自己的清单里放过的东西，必须能从自己的清单里拿走。
    pub removable: bool,

    // ── 观测侧｜权威源 = Endpoint 的两条 watch ──
    /// 传输层是否已连上。只覆盖「已建立」——内核不外露在途拨号，所以没有 `Dialing` 态。
    ///
    /// **刻意没有 `rtt_ms`。** RTT 由 ping 更新（30s/peer），而 `PingSuccess` 已被
    /// 降级成只推设备列表——它改不动 `NetworkStatus` 的任何其它字段，全量推送不值当。
    /// 真把 rtt 挂进来，这一格就会停在首帧的 `null`（`PeerConnected` 那一刻还没测过），
    /// 只在别的事件碰巧触发时才刷新一次：一个偶尔更新的延迟数比没有更误导
    /// （`DESIGN.md:645-649` 的同一条规则）。要显示 relay 延迟，先给它一条自己的
    /// 刷新通道，别顺手挂在这个读模型上。
    pub connected: bool,
    /// `None` = 这条关系在内核里没有 relay 轨道（不承担该角色，或被 `excluded` 拦下）。
    pub relay: Option<RelayLinkState>,

    // ── 策略侧｜权威源 = InfraSupervisor ──
    /// 本次节点会话内是否曾建立过 reservation。宽限期的唯一开关。
    pub ever_active: bool,
    /// 非 `None` ⇒ 当前不参与 relay 收敛。
    pub excluded: Option<InfraExclusion>,
}

/// 现场 join 三个权威源，产出全量 [`InfraLink`]。
///
/// 候选数量级是个位数（学习型候选有上限 + host 配置几条），全量重算的成本可忽略。
pub fn build_infra_links<T>(shared: &SharedNetRefs<T>) -> Vec<InfraLink> {
    let Ok(candidates) = shared.candidates.read() else {
        return Vec::new();
    };
    let snapshot = candidates.snapshot();
    drop(candidates);

    // 两条 watch 各读一次快照：逐条 link 里再各读一遍会做 N 次深拷贝。
    let relays = shared.endpoint.watch_relays().get();
    let conns = shared.endpoint.watch_conns().get();

    snapshot
        .into_iter()
        .map(|c| {
            let conn = conns.get(&c.peer_id);
            // 判据向 supervisor 要，不在这里重写一份反义的——见 `exclusion_for` 的文档。
            let excluded = shared.infra.exclusion_for(&c);
            InfraLink {
                // 含有 host 配置来源即可移除——见字段文档，这里**不能**写成 `all`。
                removable: c
                    .sources
                    .iter()
                    .any(|s| matches!(s, BootstrapCandidateSource::HostConfigured)),
                connected: conn.is_some(),
                // `excluded` 时一律给 `None`——与字段文档一致（`None` = 没有 relay 轨道，
                // 含「被闸门拦下」）。内核那张 map 里可能还留着上一轮的条目（撤销与
                // 反向收敛之间有 1s tick 的窗口），照抄出去就会发出
                // `{ excluded: 公网可达性已关, relay: failed }` 这种自相矛盾的组合，
                // 逼三端各自发明优先级，而最自然的读法（「中继正在失败」）恰恰是
                // `InfraExclusion` 文档明令禁止的呈现。
                relay: relays.get(&c.peer_id).filter(|_| excluded.is_none()).map(
                    |state| match state {
                        RelayState::Connecting => RelayLinkState::Connecting,
                        RelayState::Active { circuit_addr } => RelayLinkState::Active {
                            circuit_addr: circuit_addr.clone(),
                        },
                        RelayState::Failed { last_error } => RelayLinkState::Failed {
                            last_error: last_error.clone(),
                        },
                    },
                ),
                ever_active: shared.infra.ever_active(c.peer_id),
                excluded,
                peer_id: c.peer_id,
                addrs: c.addrs,
                sources: c.sources,
                roles: c.roles,
                scope: c.scope,
                first_seen: c.first_seen,
                last_seen: c.last_seen,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::DiscoveryMode;
    use crate::network::candidates::BootstrapCandidateManager;

    fn manager() -> BootstrapCandidateManager {
        BootstrapCandidateManager::new(DiscoveryMode::Auto, true)
    }

    fn peer() -> NodeId {
        swarmdrop_net::SecretKey::generate().node_id()
    }

    fn removable_of(mgr: &BootstrapCandidateManager, id: NodeId) -> bool {
        let c = mgr
            .snapshot()
            .into_iter()
            .find(|c| c.peer_id == id)
            .expect("候选应在表中");
        c.sources
            .iter()
            .any(|s| matches!(s, BootstrapCandidateSource::HostConfigured))
    }

    /// 用户配过的节点，连上之后**仍然**删得掉。
    ///
    /// 判据写成「全是 HostConfigured」时这条会红：`learn_candidate` 给每个
    /// `is_bootstrap_agent` 的对端补一条 `Learned`，而本仓自建的那台正是 bootstrap agent。
    /// 后果是移除按钮在它连上几秒后消失，用户自加的中继则永久删不掉、偏好还在回放。
    #[test]
    fn a_host_configured_node_stays_removable_after_it_is_also_learned() {
        let mut mgr = manager();
        let id = peer();
        let addr: Addr = "/ip4/203.0.113.7/tcp/4001".parse().unwrap();

        mgr.upsert(
            id,
            vec![addr.clone()],
            BootstrapCandidateSource::HostConfigured,
            CandidateRoles::kad_and_relay(),
        );
        assert!(removable_of(&mgr, id), "刚配上就该可移除");

        // identify 认出它是 bootstrap agent → 追加 Learned 来源
        mgr.upsert(
            id,
            vec![addr],
            BootstrapCandidateSource::Learned,
            CandidateRoles::kad_and_relay(),
        );
        assert!(
            removable_of(&mgr, id),
            "自动来源叠加上来之后，用户仍必须能把自己配的那条拿走"
        );
    }

    /// 纯自动来源不给移除入口：撤了会被下一次 identify 原样登记回来，
    /// 而撤销本身会断开全部连接（含在途传输）。
    #[test]
    fn a_purely_discovered_node_is_not_removable() {
        let mut mgr = manager();
        let id = peer();
        mgr.upsert(
            id,
            vec!["/ip4/192.168.7.7/tcp/4001".parse().unwrap()],
            BootstrapCandidateSource::MdnsLanHelper,
            CandidateRoles::kad_and_relay(),
        );
        assert!(!removable_of(&mgr, id));
    }
}
