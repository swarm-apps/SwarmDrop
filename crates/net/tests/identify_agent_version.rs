//! 运行期改写 identify 的 agent_version：已连接的对端在秒级内看到新值，
//! 两端都不重启、连接不断。这是 `live-device-rename` 在内核层的核心验收。

mod common;

use std::time::Duration;

use common::{spawn_node, spawn_node_with_agent};
use swarmdrop_net::{Events, NetEvent, NodeAddr, NodeId};

/// 主动 push 的到达本是一个 RTT 的事，给到 5s 只为吸收 CI 抖动。
/// 它同时是与「等周期交换」的分界线——周期交换要 5 分钟，超时即说明 push 没走通。
const PUSH_WINDOW: Duration = Duration::from_secs(5);

/// 幂等用例的静默观察窗口：这段时间内不应有任何来自对端的 identify 到达。
const QUIET_WINDOW: Duration = Duration::from_secs(2);

/// 首次 identify 要等连接建立与协议协商，窗口放宽。
const FIRST_IDENTIFY_WINDOW: Duration = Duration::from_secs(10);

/// 等下一条来自 `node` 的 `PeerIdentified` 并返回其 agent；窗口内没有则返回 `None`。
async fn next_agent(events: &mut Events, node: NodeId, within: Duration) -> Option<String> {
    tokio::time::timeout(within, async {
        loop {
            let ev = events.recv().await.expect("event stream closed");
            if let NetEvent::PeerIdentified {
                node: from, agent, ..
            } = ev
                && from == node
            {
                return agent;
            }
        }
    })
    .await
    .ok()
}

#[tokio::test]
async fn agent_version_update_reaches_connected_peer() {
    let (a, _) = spawn_node_with_agent("swarmdrop-test/before").await;
    let (b, b_addrs) = spawn_node().await;
    let mut b_events = b.subscribe().await.expect("subscribe");
    let a_id = a.node_id();

    a.connect(NodeAddr::with_addrs(b.node_id(), b_addrs))
        .await
        .expect("connect");

    // 首次交换：B 看到的是构造期初值
    let before = next_agent(&mut b_events, a_id, FIRST_IDENTIFY_WINDOW)
        .await
        .expect("first identify");
    assert_eq!(before, "swarmdrop-test/before");

    a.set_agent_version("swarmdrop-test/after".into())
        .await
        .expect("set agent version");

    let after = next_agent(&mut b_events, a_id, PUSH_WINDOW)
        .await
        .expect("对端应在秒级内收到第二条 PeerIdentified —— 超时说明主动 push 没走通");
    assert_eq!(after, "swarmdrop-test/after");

    // 全程不重启、不断连：这正是本 change 相对「重启节点」方案的价值
    assert!(a.is_connected(b.node_id()));

    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn same_agent_version_does_not_notify_peer() {
    const AGENT: &str = "swarmdrop-test/stable";

    let (a, _) = spawn_node_with_agent(AGENT).await;
    let (b, b_addrs) = spawn_node().await;
    let mut b_events = b.subscribe().await.expect("subscribe");
    let a_id = a.node_id();

    a.connect(NodeAddr::with_addrs(b.node_id(), b_addrs))
        .await
        .expect("connect");

    let first = next_agent(&mut b_events, a_id, FIRST_IDENTIFY_WINDOW)
        .await
        .expect("first identify");
    assert_eq!(first, AGENT);

    // 同值调用是空操作：不下发、不推送、不产生任何网络流量
    a.set_agent_version(AGENT.into())
        .await
        .expect("set agent version");

    assert!(
        next_agent(&mut b_events, a_id, QUIET_WINDOW)
            .await
            .is_none(),
        "同值调用不应推送；周期交换是 5 分钟，这个窗口内不会有别的 identify 到达"
    );

    a.close().await;
    b.close().await;
}
