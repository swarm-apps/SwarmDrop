//! 测试共用：起本地节点、等事件。
#![allow(dead_code, reason = "各集成测试二进制只用到子集")]

use std::time::Duration;

use swarmdrop_net::{Addr, Endpoint, Events, NetEvent};

/// 幂等初始化测试日志（`RUST_LOG` 控制）。
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

/// 等某 Endpoint 的监听地址就绪（端口 0 由 OS 分配，须等 watch_addrs 回填），10s 超时。
pub async fn wait_listen_addrs(endpoint: &Endpoint) -> Vec<Addr> {
    let mut watcher = endpoint.watch_addrs();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let info = watcher.get();
            if !info.listen.is_empty() {
                return info.listen;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("listen addr timeout")
}

/// 起一个只监听 127.0.0.1 TCP 的节点，返回 (endpoint, 就绪的监听地址)。
pub async fn spawn_node() -> (Endpoint, Vec<Addr>) {
    init_tracing();
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .bind()
        .await
        .expect("bind");
    let addrs = wait_listen_addrs(&endpoint).await;
    (endpoint, addrs)
}

/// 等待满足条件的事件（跳过无关事件），10s 超时。
pub async fn wait_event(events: &mut Events, pred: impl Fn(&NetEvent) -> bool) -> NetEvent {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ev = events.recv().await.expect("event stream closed");
            if pred(&ev) {
                return ev;
            }
        }
    })
    .await
    .expect("timeout waiting for event")
}
