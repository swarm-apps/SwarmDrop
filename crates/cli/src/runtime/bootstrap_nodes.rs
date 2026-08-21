//! 引导 / 中继节点清单：内置常量 + 用户的增删。
//!
//! **内置那份是部署配置，不属于 P2P 内核**——核心不持有任何公共基础设施地址，各宿主按
//! 自身 transport 能力提供各自的清单。三端各有一份：桌面 `src/lib/bootstrap-nodes.ts`、
//! 移动 `mobile/src/core/bootstrap-nodes.ts`、浏览器 `docs/app/app/_lib/relay-helpers.ts`。
//!
//! 命令行宿主与桌面、移动同属原生端，可用 transport 相同，故内置清单与它们一致。
//! **浏览器那份不能照抄**：它列的是 webrtc-direct / WebTransport 地址，原生端用不上，
//! 而原生端的裸 TCP 地址浏览器也拨不通。
//!
//! ## 合并规则
//!
//! ```text
//! 生效清单 = (内置清单 − removed) ∪ custom
//! ```
//!
//! ⚠️ **增删刻意不对称**（见 [`add`] 与 [`remove`]）。把「撤销一条内置项」实现成「把内置
//! 清单复制进 custom 再删一条」会得到一个已知故障：版本更新换掉内置地址时，老用户的
//! custom 里躺着一份旧地址快照，新地址永远到不了他手上，而故障形态是「升级后突然连不上」
//! 且无法自查。判据与 Web 端同一条（spec: `bootstrap-node-settings`）。
//!
//! ## 允许清空到零条
//!
//! 不设下限。只在局域网内用 SwarmDrop 是合理场景，而一个「至少留一条」的下限会在用户想
//! 换掉全部内置节点时挡住他（他得先加后删，顺序错了就被拒）。代价是用户可能把自己配到
//! 连不上公网——由 `status` 与 `bootstrap list` 如实显示承担，不由写入路径拦。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use swarmdrop_core::infra::{InfraAddrError, InfraLink, validate_shape};
use swarmdrop_core::network::NetworkRuntimeConfig;
use swarmdrop_core::network::candidates::CandidateRoles;
use swarmdrop_net::{Addr, NodeAddr, NodeId};

use crate::exit::{CliError, CliResult};

use super::access::Records;
use super::boot::RunningNode;
use super::settings::BootstrapOverlay;

/// 自建引导节点（同时是中继）。
///
/// **随版本更新，且用户的增删是对它的叠加而不是快照**——见模块文档的合并规则。
const BUILTIN: &[&str] = &[
    "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
    "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/// 一条引导节点是内置的还是用户加的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Builtin,
    Custom,
}

/// 清单里的一条。
///
/// `Deserialize` 是本地通道要的：清单可能由常驻节点给出，而渲染发生在发起命令的那个
/// 进程里。两条取数路径于是产出同一个类型，渲染层不必分辨它从哪来。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapRow {
    pub addr: String,
    pub peer_id: String,
    pub origin: Origin,
    /// 能否从本机清单里移除。
    ///
    /// 配置清单里的每一条都可以——内置项撤销后进 `removed` 集合，不是「删不掉」。
    /// 字段仍然给出来，是因为消费方不该把这条判断硬编码成 `true`：将来若把 identify
    /// 学到的候选也列进来，它们就不可移除（撤销会断开与该节点的全部连接，而那可能是
    /// 一台正在传文件的已配对设备）。
    pub removable: bool,
    /// 节点在跑时的链路状态；没有节点在跑时为 `None`。
    ///
    /// 复用 core 的读模型而不是自己压扁一遍：`last_error` 原文是三端唯一能说清「为什么
    /// 连不上」的东西，压扁一次就少一次。
    ///
    /// ⚠️ 它是**按 peer** 而不是按地址的：同一节点的 TCP 与 QUIC 两条地址会带同一份
    /// 链路状态，那是事实——基础设施关系本来就建立在节点之间，不在地址之间。
    pub link: Option<InfraLink>,
}

/// 一次增删的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapChanged {
    /// 被加上/撤掉的那条（canonical 形式）。
    pub addr: String,
    /// 改完之后生效清单里还有几条。**清零是合法的**，见模块文档。
    pub remaining: usize,
    /// 这次改动是当场生效了，还是要等下次启动。
    pub effect: super::settings::scalar::Effect,
}

/// 生效清单：`(内置 − removed) ∪ custom`，保留「内置在前」的顺序。
///
/// 去重是防御性的：命令面不会产出重复（[`add`] 会拒），但手改过配置文件的机器不该因此
/// 让节点多拨一次同一个地址。
pub fn effective(overlay: &BootstrapOverlay) -> Vec<String> {
    let removed: BTreeSet<&str> = overlay.removed.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    BUILTIN
        .iter()
        .map(|addr| (*addr).to_owned())
        .filter(|addr| !removed.contains(addr.as_str()))
        .chain(overlay.custom.iter().cloned())
        .filter(|addr| seen.insert(addr.clone()))
        .collect()
}

/// 命令行宿主的网络配置。
///
/// `provide_lan_helper` 保持关闭：局域网协助节点会为同网设备转发流量，那是用户该显式
/// 选择的事，不该因为「跑了个命令行」就默默承担。
pub fn network_config(overlay: &BootstrapOverlay) -> NetworkRuntimeConfig {
    NetworkRuntimeConfig {
        bootstrap_nodes: effective(overlay),
        ..Default::default()
    }
}

/// 生效清单，逐条带上来源与（有节点时）链路状态。
pub async fn list(records: &Records, node: Option<&RunningNode>) -> CliResult<Vec<BootstrapRow>> {
    let overlay = records.settings().read()?.bootstrap;
    let links = links_by_peer(node);

    Ok(effective(&overlay)
        .into_iter()
        .map(|addr| {
            let peer_id = parse(&addr).map(|peer| peer.id);
            BootstrapRow {
                origin: origin_of(&addr),
                removable: true,
                link: peer_id.as_ref().and_then(|id| links.get(id).cloned()),
                peer_id: peer_id.map(|id| id.to_string()).unwrap_or_default(),
                addr,
            }
        })
        .collect())
}

/// 添加一条。
///
/// 撤销过的内置项**从 `removed` 里拿回来**，而不是往 `custom` 里塞一份副本——后者会让
/// 那条地址在版本更新换掉内置地址之后仍然指向旧的那个。
pub async fn add(
    records: &Records,
    node: Option<&RunningNode>,
    input: &str,
) -> CliResult<BootstrapChanged> {
    let overlay = records.settings().read()?.bootstrap;
    let before = effective(&overlay);

    // 校验：形状四条规则 + 与**本机清单**去重。
    //
    // ⚠️ 去重比的是本机清单而不是节点的候选表：候选表里还有 mDNS 与 identify 学来的
    // 候选，拿它当判据会让用户加不进一条他明明还没加过的地址（而且那条学来的候选下次
    // 重启就没了，他的清单里却什么也没留下）。
    let peer = validate(records, node, input).await?;
    let addr = canonical(&peer);
    if before.contains(&addr) {
        return Err(refused(InfraAddrError::Duplicate, input));
    }

    let stored = records.settings().update(|settings| {
        let overlay = &mut settings.bootstrap;
        if let Some(index) = overlay.removed.iter().position(|had| *had == addr) {
            // 撤销过的内置项：把它放回来。
            overlay.removed.remove(index);
        } else {
            overlay.custom.push(addr.clone());
        }
        Ok(())
    })?;

    let after = effective(&stored.bootstrap);
    let effect = apply_to_node(node, &before, &after).await?;
    Ok(BootstrapChanged {
        addr,
        remaining: after.len(),
        effect,
    })
}

/// 撤销一条。
///
/// 内置项进 `removed`，自定义项从 `custom` 拿走——这两条不对称是必须的，见模块文档。
pub async fn remove(
    records: &Records,
    node: Option<&RunningNode>,
    input: &str,
) -> CliResult<BootstrapChanged> {
    let overlay = records.settings().read()?.bootstrap;
    let before = effective(&overlay);

    // 用户敲的可能是另一种等价写法，所以先归一化再在清单里定位；归一化不了就按原样找
    // （手改过配置文件的机器上，`custom` 里可能躺着一条解析不动的字符串，那也得删得掉）。
    let addr = parse(input.trim())
        .as_ref()
        .map(canonical)
        .unwrap_or_else(|| input.trim().to_owned());
    if !before.contains(&addr) {
        return Err(CliError::Usage(format!(
            "清单里没有这条引导节点: {input}\n用 swarmdrop bootstrap list 看有哪些。"
        )));
    }

    let stored = records.settings().update(|settings| {
        let overlay = &mut settings.bootstrap;
        match overlay.custom.iter().position(|had| *had == addr) {
            Some(index) => {
                overlay.custom.remove(index);
            }
            // 不在 custom 里 ⇒ 它是内置项，记一笔「撤销」而不是复制整份清单出来编辑。
            None => overlay.removed.push(addr.clone()),
        }
        Ok(())
    })?;

    let after = effective(&stored.bootstrap);
    let effect = apply_to_node(node, &before, &after).await?;
    Ok(BootstrapChanged {
        addr,
        remaining: after.len(),
        effect,
    })
}

/// 提交前的同步校验，**零网络往返**。
///
/// 「能不能连上」不在这里回答——那由提交后的收敛环给（`InfraLink` 会转成失败态并带上
/// 内核下发的原文）。这里只挡 100% 确定的形状错误。
///
/// 两个输入的来源随「有没有节点」而变，但**判据是同一个函数**：
///
/// | | 本机身份 | 本端装配了哪些传输 |
/// |---|---|---|
/// | 有节点 | 节点的 `node_id` | `Endpoint::supported_transports()` |
/// | 无节点 | 身份文件（**读不出就跳过这条规则**） | `planned_transports(Native)` |
///
/// 无身份时跳过「指向本机」是正确的而不是将就：全新机器上还没有本机身份，节点下次启动
/// 会随机生成一个，它不可能与用户此刻粘的地址相同。**不能改成「读不出就现场创建一个」**
/// ——那会让一条只读本机记录的命令产出身份文件，而 `tests/without_a_node.rs` 正是用它
/// 的存在与否判断「有没有起过节点」。
async fn validate(
    records: &Records,
    node: Option<&RunningNode>,
    input: &str,
) -> CliResult<NodeAddr> {
    let (self_id, supported) = match node {
        Some(node) => (
            Some(node.node_id),
            node.manager.endpoint().supported_transports().to_vec(),
        ),
        None => (
            records.local_node_id().await?,
            swarmdrop_core::runtime::planned_transports(
                swarmdrop_core::runtime::EndpointProfile::Native,
            ),
        ),
    };

    validate_shape(input, self_id, &supported).map_err(|err| refused(err, input))
}

/// 把校验失败翻成一句用户能据以行动的话。
///
/// 原样把 `InfraAddrError` 的 Debug 甩出去是不行的：它的 `UnsupportedTransport` 带着
/// 本端支持什么，而那正是用户下一步要看的东西。
fn refused(err: InfraAddrError, input: &str) -> CliError {
    let detail = match err {
        InfraAddrError::Malformed { detail } => format!("不是合法的 multiaddr: {detail}"),
        InfraAddrError::MissingPeerId => {
            "缺少 /p2p/<节点标识> 段——没有身份就无法验证连上的是不是同一台".to_owned()
        }
        InfraAddrError::NoTransport => "地址里没有可拨的传输段".to_owned(),
        InfraAddrError::UnsupportedTransport {
            transport,
            supported,
        } => format!(
            "本端没有装配 {transport} 传输；本端支持: {}",
            supported.join(" / ")
        ),
        InfraAddrError::SelfAddr => "这是本机的地址".to_owned(),
        InfraAddrError::Duplicate => "清单里已经有这条了".to_owned(),
    };
    CliError::Usage(format!("引导节点地址被拒: {detail}\n  {input}"))
}

/// 把持久化清单的一次改动落到正在运行的节点上。
///
/// **两步而不是只加或只减**：候选表的 upsert 对地址是**累加**，没有「去掉一条地址」的
/// 原语——同一个 peer 有多条地址时（内置的 TCP + QUIC 就是），撤掉其中一条只能整条撤销
/// 再按剩余地址重新登记。
///
/// 没有节点时**什么都不做，更不会为此拉起一个**：改动已经落盘，下次启动时由
/// [`network_config`] 回放（spec: `bootstrap-node-settings` 的「命令行宿主无节点时增删」）。
async fn apply_to_node(
    node: Option<&RunningNode>,
    before: &[String],
    after: &[String],
) -> CliResult<super::settings::scalar::Effect> {
    use super::settings::scalar::Effect;

    let Some(node) = node else {
        return Ok(Effect::PendingStart);
    };

    let before = by_peer(before);
    let after = by_peer(after);

    for (peer, addrs) in &before {
        if after.get(peer) != Some(addrs) {
            node.manager
                .remove_infra_intent(*peer)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("撤销引导节点意图失败: {err}")))?;
        }
    }
    for (peer, addrs) in after {
        // 角色给全：手动配的引导节点既当 DHT 种子也当中继候选。真正是否建 reservation
        // 由 supervisor 按 `public_reachability` 闸门决定，不在这里预判。
        node.manager.ensure_infra_intent(
            NodeAddr { id: peer, addrs },
            CandidateRoles::kad_and_relay(),
        );
    }
    Ok(Effect::Applied)
}

/// 把地址清单按 peer 归并——基础设施关系建立在节点之间，不在地址之间。
///
/// 值是 `Vec` 而不是集合：`Addr` 只有 `Hash + Eq`（libp2p 的 `Multiaddr` 没有 `Ord`），
/// 而登记时的地址顺序最好是稳定的。顺序直接继承生效清单，逐条去重。
fn by_peer(addrs: &[String]) -> BTreeMap<NodeId, Vec<Addr>> {
    let mut grouped: BTreeMap<NodeId, Vec<Addr>> = BTreeMap::new();
    for raw in addrs {
        if let Some(peer) = parse(raw) {
            let slot = grouped.entry(peer.id).or_default();
            for addr in peer.addrs {
                if !slot.contains(&addr) {
                    slot.push(addr);
                }
            }
        }
    }
    grouped
}

/// 节点此刻的基础设施关系，按 peer 索引。没有节点时为空表。
fn links_by_peer(node: Option<&RunningNode>) -> BTreeMap<NodeId, InfraLink> {
    let Some(node) = node else {
        return BTreeMap::new();
    };
    swarmdrop_core::infra::build_infra_links(&node.manager.shared_refs())
        .into_iter()
        .map(|link| (link.peer_id, link))
        .collect()
}

/// 一条地址的 canonical 形式——**存进配置文件的就是它**。
///
/// 同一条 multiaddr 有多种等价写法，按用户敲的原样存会让 `remove` 认不出自己刚 `add`
/// 进去的那条。
fn canonical(peer: &NodeAddr) -> String {
    peer.addrs
        .first()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// 解析成 `(节点标识, 地址)`；解析不动时 `None`（手改过配置文件的机器上会出现）。
fn parse(raw: &str) -> Option<NodeAddr> {
    let addr: Addr = raw.trim().parse().ok()?;
    let id = addr.p2p_node_id()?;
    Some(NodeAddr {
        id,
        addrs: vec![addr],
    })
}

fn origin_of(addr: &str) -> Origin {
    if BUILTIN.contains(&addr) {
        Origin::Builtin
    } else {
        Origin::Custom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHER: &str =
        "/ip4/198.51.100.7/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";

    fn overlay(custom: &[&str], removed: &[&str]) -> BootstrapOverlay {
        BootstrapOverlay {
            custom: custom.iter().map(|s| (*s).to_owned()).collect(),
            removed: removed.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// 内置清单里每条都必须能解析出节点标识——地址写错时，症状是「连不上引导节点」，
    /// 而那与网络故障无法区分，只有这条测试能在提交前拦住它。
    #[test]
    fn every_builtin_address_parses() {
        assert!(!BUILTIN.is_empty(), "内置清单不得为空");
        for raw in BUILTIN {
            let addr: Addr = raw.parse().expect("引导地址无法解析为 multiaddr");
            assert!(
                addr.p2p_node_id().is_some(),
                "引导地址缺少 /p2p 段，拨过去无法校验对端身份: {raw}"
            );
        }
    }

    /// **内置清单必须写成 canonical 形式。**
    ///
    /// 存进配置文件的是解析后再打印出来的那一串（见 [`canonical`]）。内置的写法若与它
    /// 不同，`removed` 里记下的字符串就永远匹配不上内置项——撤销一条内置节点会「成功」
    /// 但下次启动它照样回来。
    #[test]
    fn builtin_addresses_are_already_canonical() {
        for raw in BUILTIN {
            let parsed = parse(raw).expect("可解析");
            assert_eq!(&canonical(&parsed), raw, "内置地址不是 canonical 写法");
        }
    }

    /// 没做过任何增删的用户拿到的就是内置清单本身。
    #[test]
    fn an_untouched_overlay_is_exactly_the_builtin_list() {
        assert_eq!(effective(&BootstrapOverlay::default()), BUILTIN);
    }

    /// **升级换了内置地址，老用户拿得到新的。**
    ///
    /// 这条钉住整个两集合模型的存在理由：持久化的是「用户做了什么」，不是「合并后的
    /// 清单长什么样」。加过一条自定义节点的用户，内置那几条仍然随版本更新。
    #[test]
    fn a_user_with_custom_entries_still_gets_new_builtins() {
        let list = effective(&overlay(&[OTHER], &[]));
        for builtin in BUILTIN {
            assert!(
                list.contains(&(*builtin).to_owned()),
                "丢了内置项 {builtin}"
            );
        }
        assert!(list.contains(&OTHER.to_owned()));
    }

    /// 撤销过的内置项不出现在清单里，其余内置项不受影响。
    #[test]
    fn a_revoked_builtin_disappears_and_the_rest_stay() {
        let list = effective(&overlay(&[], &[BUILTIN[0]]));
        assert!(!list.contains(&BUILTIN[0].to_owned()));
        assert!(list.contains(&BUILTIN[1].to_owned()));
    }

    /// 允许清空到零条——不设下限，理由见模块文档。
    #[test]
    fn the_list_may_be_emptied_completely() {
        assert!(effective(&overlay(&[], BUILTIN)).is_empty());
    }

    /// 归并按 peer：内置那两条是同一个节点的两条路径。
    #[test]
    fn addresses_of_one_node_group_into_one_relationship() {
        let grouped = by_peer(&effective(&BootstrapOverlay::default()));
        assert_eq!(grouped.len(), 1, "内置那两条是同一个节点");
        assert_eq!(grouped.values().next().map(Vec::len), Some(2));
    }

    /// 解析不动的条目不该让整个清单塌掉——手改过配置文件的机器上会出现。
    #[test]
    fn an_unparseable_entry_is_skipped_not_fatal() {
        let grouped = by_peer(&["not-a-multiaddr".to_owned(), BUILTIN[0].to_owned()]);
        assert_eq!(grouped.len(), 1);
    }

    #[test]
    fn origin_tells_builtin_from_custom() {
        assert_eq!(origin_of(BUILTIN[0]), Origin::Builtin);
        assert_eq!(origin_of(OTHER), Origin::Custom);
    }

    /// 一个空数据目录上的 [`Records`]。返回的 `TempDir` 是 RAII guard，
    /// 调用点必须把它绑到一个活到用例结束的名字上。
    fn fixture() -> (tempfile::TempDir, Records) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");
        (tmp, Records::new(dir))
    }

    fn stored(records: &Records) -> BootstrapOverlay {
        records.settings().read().expect("读配置").bootstrap
    }

    /// 无节点时添加：落盘、报「待下次启动」，**且不产出身份文件**。
    ///
    /// 后半句是「不为写入拉起节点」这条的可判定形态——`identity.json` 只有装配节点时
    /// 才会被创建出来。
    #[tokio::test]
    async fn adding_without_a_node_persists_and_starts_nothing() {
        let (tmp, records) = fixture();

        let changed = add(&records, None, OTHER).await.expect("添加");

        assert_eq!(changed.addr, OTHER);
        assert_eq!(
            changed.effect,
            crate::runtime::settings::scalar::Effect::PendingStart
        );
        assert_eq!(stored(&records).custom, [OTHER]);
        assert!(
            !tmp.path().join("identity.json").exists(),
            "写一条配置不该把节点装配起来"
        );
    }

    /// **撤销一条内置项记的是「撤销」，不是把内置清单复制出来编辑。**
    ///
    /// 这条钉住两集合模型：`custom` 必须仍然是空的，否则老用户会在下次内置地址更新时
    /// 被永久压在旧地址上。
    #[tokio::test]
    async fn revoking_a_builtin_records_a_revocation_not_a_snapshot() {
        let (_tmp, records) = fixture();

        remove(&records, None, BUILTIN[0]).await.expect("撤销");

        let overlay = stored(&records);
        assert_eq!(overlay.removed, [BUILTIN[0]]);
        assert!(
            overlay.custom.is_empty(),
            "撤销内置项不得把内置清单复制进 custom"
        );
        assert!(!effective(&overlay).contains(&BUILTIN[0].to_owned()));
    }

    /// 把撤销过的内置项加回来，是从 `removed` 里拿走它，而不是往 `custom` 里塞一份副本。
    #[tokio::test]
    async fn re_adding_a_revoked_builtin_takes_it_out_of_removed() {
        let (_tmp, records) = fixture();

        remove(&records, None, BUILTIN[0]).await.expect("撤销");
        add(&records, None, BUILTIN[0]).await.expect("加回");

        let overlay = stored(&records);
        assert!(overlay.removed.is_empty(), "撤销记录应当被撤掉");
        assert!(overlay.custom.is_empty(), "内置项不该跑进 custom");
        assert_eq!(effective(&overlay), BUILTIN);
    }

    /// 撤销自定义项是从 `custom` 里拿走，**不往 `removed` 里加**——那里只装内置项。
    #[tokio::test]
    async fn revoking_a_custom_entry_removes_it_from_custom() {
        let (_tmp, records) = fixture();

        add(&records, None, OTHER).await.expect("添加");
        remove(&records, None, OTHER).await.expect("撤销");

        let overlay = stored(&records);
        assert!(overlay.custom.is_empty());
        assert!(
            overlay.removed.is_empty(),
            "removed 只装内置项；自定义项进去会在清单里留下一条永远撤不掉的幽灵"
        );
    }

    /// 清单可以被清空到零条，且那**不是**错误。
    #[tokio::test]
    async fn the_list_can_be_emptied_through_the_command_path() {
        let (_tmp, records) = fixture();

        for builtin in BUILTIN {
            remove(&records, None, builtin).await.expect("撤销");
        }
        let last = remove(&records, None, BUILTIN[0]).await;
        assert!(last.is_err(), "已经撤过的那条不该还能再撤一次");
        assert!(effective(&stored(&records)).is_empty());
    }

    /// 形状不对的地址当场被拒，且**什么都不写**。
    #[tokio::test]
    async fn a_malformed_address_is_refused_and_writes_nothing() {
        let (_tmp, records) = fixture();

        for bad in [
            "not a multiaddr",
            // 缺 /p2p/：没有身份就无法验证连上的是不是同一台
            "/ip4/198.51.100.7/tcp/4001",
            // 只有身份没有传输段
            "/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
        ] {
            let err = add(&records, None, bad).await.expect_err("{bad} 应当被拒");
            assert_eq!(err.code(), crate::exit::Code::Usage, "{bad}");
        }
        assert_eq!(stored(&records), BootstrapOverlay::default());
    }

    /// 已经在清单里的地址加不进第二遍。
    #[tokio::test]
    async fn adding_something_already_on_the_list_is_refused() {
        let (_tmp, records) = fixture();

        let err = add(&records, None, BUILTIN[0])
            .await
            .expect_err("内置项已经在清单里");
        assert_eq!(err.code(), crate::exit::Code::Usage);

        add(&records, None, OTHER).await.expect("首次添加");
        let err = add(&records, None, OTHER)
            .await
            .expect_err("第二次应当被拒");
        assert_eq!(err.code(), crate::exit::Code::Usage);
        assert_eq!(stored(&records).custom.len(), 1);
    }

    /// 清单里没有的地址撤不掉——报错而不是静默成功。
    #[tokio::test]
    async fn removing_something_absent_is_an_error() {
        let (_tmp, records) = fixture();
        let err = remove(&records, None, OTHER).await.expect_err("不该成功");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    /// 无节点时的清单：逐条带来源，链路状态一律「未知」而不是「连不上」。
    #[tokio::test]
    async fn listing_without_a_node_reports_unknown_links() {
        let (_tmp, records) = fixture();

        let rows = list(&records, None).await.expect("清单");
        assert_eq!(rows.len(), BUILTIN.len());
        assert!(rows.iter().all(|row| row.origin == Origin::Builtin));
        assert!(
            rows.iter().all(|row| row.link.is_none()),
            "没有节点时不得编造链路状态"
        );
        assert!(rows.iter().all(|row| !row.peer_id.is_empty()));
    }
}
