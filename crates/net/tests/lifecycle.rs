//! 生命周期契约：close 后的 API 行为、clone 共享关停、入站流配额拒绝。

mod common;

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::spawn_node;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, ConnectError, Endpoint, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router,
    SecretKey, StreamLimits,
};

const HOLD: ProtocolId = ProtocolId::from_static("/test/hold/1");

/// 读到 EOF 才回一个字节的 handler——用于占住入站配额。
#[derive(Debug, Clone)]
struct HoldEcho;

impl ProtocolHandler for HoldEcho {
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;
        stream.write_all(&[1]).await?;
        stream.close().await?;
        Ok(())
    }
}

#[tokio::test]
async fn closed_endpoint_rejects_all_operations() {
    let (a, _) = spawn_node().await;
    let (b, b_addrs) = spawn_node().await;
    let a_clone = a.clone();

    a.close().await;

    // clone 共享同一 actor——一处 close 处处失效
    assert!(
        a_clone
            .connect(NodeAddr::with_addrs(b.node_id(), b_addrs))
            .await
            .is_err(),
        "closed endpoint 的 connect 必须失败"
    );
    assert!(a_clone.open(b.node_id(), HOLD).await.is_err());
    assert!(a_clone.subscribe().await.is_err());
    assert!(a_clone.add_addrs(b.node_id(), vec![]).await.is_err());

    // close 幂等
    a_clone.close().await;

    // closed() 信号已 resolve
    tokio::time::timeout(Duration::from_secs(1), a.closed())
        .await
        .expect("closed() should resolve after close()");

    b.close().await;
}

#[tokio::test]
async fn subscriber_stream_ends_on_close() {
    let (a, _) = spawn_node().await;
    let mut events = a.subscribe().await.expect("subscribe");
    a.close().await;
    // actor 停止 → 事件流结束（不是挂死）
    let end = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event stream must end, not hang");
    assert!(end.is_none());
}

/// connect 超时不只是丢掉 Future：第二次同 peer 拨号必须能新建 TCP 连接。
/// 否则会复用第一次遗留的 pending dial（libp2p 的 DialPeerConditionFalse）。
#[tokio::test]
async fn connect_timeout_aborts_orphaned_dial() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind hanging peer");
    listener
        .set_nonblocking(true)
        .expect("make listener non-blocking");
    let port = listener.local_addr().expect("listener addr").port();
    // 存活期由测试主体显式收口，**不能用 wall-clock deadline**：这个线程从 spawn 起就开始
    // 计时，而下面的 `Endpoint::builder().bind()` 在慢机器上要好几秒（实测 5–8s）。用固定
    // 时限的话 listener 会在 client 拨号之前就退出，于是拨号拿到的是 `Connection refused`
    // 而不是本用例要断言的「挂起后超时」——测试变成了和机器速度赛跑，且失败信息完全指不到病因。
    // CI 只跑 ubuntu，那里 bind 快，所以这条只在本地慢机器上红。
    let stop = Arc::new(AtomicBool::new(false));
    let accepted = std::thread::spawn({
        let stop = Arc::clone(&stop);
        move || {
            let mut sockets = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    // 保持 TCP 打开但不参与 Noise 握手：每次 client 拨号都会留下一
                    // 条可计数连接，直到测试结束统一 drop。
                    Ok((socket, _)) => sockets.push(socket),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept hanging peer: {error}"),
                }
            }
            sockets.len()
        }
    });

    let client = Endpoint::builder()
        .connect_timeout(Duration::from_millis(100))
        .bind()
        .await
        .expect("bind client");
    let target = NodeAddr::with_addrs(
        SecretKey::generate().node_id(),
        vec![
            format!("/ip4/127.0.0.1/tcp/{port}")
                .parse()
                .expect("valid address"),
        ],
    );
    let result = client.connect(target.clone()).await;
    // 给 actor 一次 poll Swarm 的机会处理 libp2p abort；随后同 peer 再拨一次。
    tokio::time::sleep(Duration::from_millis(50)).await;
    let retry = client
        .connect_with_timeout(target, Duration::from_millis(100))
        .await;

    // 两次拨号都发出去了，让 listener 收工。这一步排在断言**之前**：断言失败时线程同样能退出，
    // 失败现场是一条清楚的断言消息，而不是一个挂住的测试进程。
    stop.store(true, Ordering::Relaxed);
    let dials = accepted.join().expect("hanging peer thread");

    assert!(
        matches!(result, Err(ConnectError::Timeout)),
        "hanging peer 应在调用方时限内超时，got: {result:?}"
    );
    assert!(
        matches!(retry, Err(ConnectError::Timeout)),
        "第二次拨号也应独立超时，got: {retry:?}"
    );
    assert!(
        dials >= 2,
        "取消旧拨号后，同 peer 的新 connect 必须发起新的 TCP 拨号，实际只有 {dials} 次"
    );

    client.close().await;
}

/// 等到外部地址视图满足 `predicate`，超时即失败。
async fn await_external<F>(
    endpoint: &Endpoint,
    what: &str,
    predicate: F,
) -> Vec<swarmdrop_net::Addr>
where
    F: Fn(&[swarmdrop_net::Addr]) -> bool,
{
    let mut watcher = endpoint.watch_addrs();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let external = watcher.get().external;
            if predicate(&external) {
                return external;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .unwrap_or_else(|_| panic!("外部地址视图始终未满足：{what}"))
}

#[tokio::test]
async fn declared_external_addresses_are_published() {
    let configured: swarmdrop_net::Addr = "/ip4/203.0.113.10/tcp/4001".parse().unwrap();
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .external_addrs(vec![configured.clone()])
        .bind()
        .await
        .expect("bind");

    let dynamic: swarmdrop_net::Addr = "/ip4/203.0.113.10/udp/4003/quic-v1".parse().unwrap();
    endpoint
        .set_external_addrs(vec![configured.clone(), dynamic.clone()])
        .await
        .expect("declare external addresses");

    let external = await_external(&endpoint, "两条声明的地址都出现", |ext| {
        ext.contains(&configured) && ext.contains(&dynamic)
    })
    .await;
    assert!(external.contains(&configured));
    assert!(external.contains(&dynamic));
    endpoint.close().await;
}

/// **本次改动的核心护栏**：整份声明里被去掉的地址必须真的从通告集合里消失。
///
/// 没有它，带 `certhash` 的地址（WebTransport / WebRTC Direct）每轮换一次就多留一条死
/// 地址，最终撑爆 identify 的 4096 字节上限——而那个上限只在**解码端**检查，本机永远
/// 不会报错，症状是每个对端都突然读不到本节点的 identify。
#[tokio::test]
async fn redeclaring_external_addresses_retracts_the_ones_left_out() {
    let keep: swarmdrop_net::Addr = "/ip4/203.0.113.10/tcp/4001".parse().unwrap();
    let stale: swarmdrop_net::Addr = "/ip4/203.0.113.10/udp/4004/quic-v1".parse().unwrap();

    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .bind()
        .await
        .expect("bind");

    endpoint
        .set_external_addrs(vec![keep.clone(), stale.clone()])
        .await
        .expect("declare both");
    await_external(&endpoint, "两条都先出现", |ext| {
        ext.contains(&keep) && ext.contains(&stale)
    })
    .await;

    // 第二次声明去掉 `stale` —— 等价于「证书轮换后旧地址失效」。
    endpoint
        .set_external_addrs(vec![keep.clone()])
        .await
        .expect("redeclare without stale");

    let external = await_external(&endpoint, "被去掉的那条消失", |ext| {
        !ext.contains(&stale)
    })
    .await;
    assert!(
        external.contains(&keep),
        "仍在声明中的地址不该被连带撤销：{external:?}"
    );
    endpoint.close().await;
}

/// `external_ip` 把**运行期才知道的**监听地址映射成公网形态。
///
/// 与 `external_addrs` 的差别正在这里：监听端口是 `tcp/0` 让内核分配的，bind 时谁都算不
/// 出来，静态声明根本没有可声明的内容。带 certhash 的传输是同一个形状——那串 hash 由
/// 传输在启动时产生，只有跟着监听地址走才拿得到当前正确的那个。
#[tokio::test]
async fn external_ip_maps_runtime_listen_addresses() {
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .external_ip("203.0.113.10".parse().expect("valid ip"))
        .bind()
        .await
        .expect("bind");

    let external = await_external(&endpoint, "出现映射到公网 IP 的地址", |ext| {
        ext.iter().any(|a| {
            let s = a.to_string();
            s.starts_with("/ip4/203.0.113.10/tcp/") && !s.ends_with("/tcp/0")
        })
    })
    .await;

    // 监听视图里那条的端口必须与映射出来的一致——映射改的只有 IP 段。
    let listen_port = endpoint
        .watch_addrs()
        .get()
        .listen
        .iter()
        .find_map(|a| {
            // `split_once` 而不是 `rsplit().next()`：后者对不含 `/tcp/` 的地址会把整条
            // 地址当成「端口」返回，于是这个 find_map 永远命中第一条、下面的 expect 形同虚设。
            a.to_string()
                .split_once("/tcp/")
                .map(|(_, port)| port.to_owned())
        })
        .expect("应有一条 tcp 监听地址");
    assert!(
        external
            .iter()
            .any(|a| a.to_string() == format!("/ip4/203.0.113.10/tcp/{listen_port}")),
        "映射结果应与实际监听端口一致：{external:?}"
    );

    endpoint.close().await;
}

/// 映射出来的地址与宿主声明的地址**并存**，互不覆盖。
///
/// 两者是两个独立来源（一个是「我确知这几条」、一个是「我的监听地址换上这个 IP」），
/// 合成一份账本的话，其中一方每更新一次就会把另一方抹掉。
#[tokio::test]
async fn external_ip_and_declared_addresses_coexist() {
    let declared: swarmdrop_net::Addr = "/ip4/203.0.113.10/udp/4001/quic-v1".parse().unwrap();
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .external_ip("203.0.113.10".parse().expect("valid ip"))
        .external_addrs(vec![declared.clone()])
        .bind()
        .await
        .expect("bind");

    let external = await_external(&endpoint, "声明的与映射的同时在场", |ext| {
        ext.contains(&declared)
            && ext
                .iter()
                .any(|a| a.to_string().starts_with("/ip4/203.0.113.10/tcp/"))
    })
    .await;
    assert!(external.contains(&declared), "实得 {external:?}");

    endpoint.close().await;
}

/// 声明是**幂等**的：同一份内容重复声明不应让视图产生新版本。
///
/// 这条看守的是调用方的重试路径 —— bootstrap 的地址跟踪任务每次 watch 醒来都会重发
/// 一次全集。若每次都判为「变了」，就会向所有订阅者与 lookup 白广播一轮。
#[tokio::test]
async fn redeclaring_the_same_addresses_is_idempotent() {
    let addr: swarmdrop_net::Addr = "/ip4/203.0.113.10/tcp/4001".parse().unwrap();
    // **刻意不监听任何地址**：`watch_addrs` 覆盖整个 `AddrsInfo`，监听地址到达同样会
    // 唤醒它。留着 listen 的话这条断言测的就不是「声明幂等」，而是「NewListenAddr 有没有
    // 恰好在这 300ms 里到」——一条会随机变红的测试。
    let endpoint = Endpoint::builder().bind().await.expect("bind");

    endpoint
        .set_external_addrs(vec![addr.clone()])
        .await
        .expect("declare");
    await_external(&endpoint, "地址出现", |ext| ext.contains(&addr)).await;

    let mut watcher = endpoint.watch_addrs();
    // ⚠️ 必须用 `updated()` 消费积压，不能用 `get()`：`get()` 走的是 `borrow()`，**不推进
    // 版本标记**。而新建的 `Watcher` 是从 Endpoint 那个从未被读过的 receiver clone 来的，
    // 它一开始就带着「有未读变更」，于是首次 `updated()` 必定立刻返回 —— 拿 `get()` 当
    // 消费手段的话，下面那条断言测的是这个继承来的标记，跟幂等性无关。
    let _ = tokio::time::timeout(Duration::from_millis(200), watcher.updated()).await;

    for _ in 0..3 {
        endpoint
            .set_external_addrs(vec![addr.clone()])
            .await
            .expect("redeclare");
    }

    // 重复声明不该产生新版本；给一点时间让「若真发了」的通知抵达。
    let woke = tokio::time::timeout(Duration::from_millis(300), watcher.updated())
        .await
        .is_ok();
    assert!(!woke, "内容相同的重复声明不应触发视图更新");
    endpoint.close().await;
}

#[tokio::test]
async fn inbound_streams_beyond_limit_are_rejected() {
    // server 每 peer 只允许 1 条入站流
    let server = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .stream_limits(StreamLimits {
            max_inbound_per_peer: 1,
            max_outbound_per_peer: 8,
            max_per_protocol: 64,
        })
        .bind()
        .await
        .expect("bind server");
    let server_addrs = common::wait_listen_addrs(&server).await;
    let router = Router::builder(server.clone())
        .accept(HOLD, HoldEcho)
        .spawn();

    let (client, _) = spawn_node().await;
    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    // 第一条流：不关写侧，长期占住配额
    let mut held = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open first");
    held.write_all(b"hold").await.expect("write");
    // （不 close——handler 的 read_to_end 挂着，配额不归还）

    // 给 Router 一点时间领走第一条流
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 第二条流：超限，Router 直接 drop——读侧表现为立即 EOF/reset
    let mut rejected = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open second (open 本身成功，拒绝发生在对端)");
    let _ = rejected.write_all(b"x").await;
    let _ = rejected.close().await;
    let mut sink = Vec::new();
    let outcome = tokio::time::timeout(Duration::from_secs(5), rejected.read_to_end(&mut sink))
        .await
        .expect("rejected stream must resolve quickly");
    assert!(
        outcome.is_err() || sink.is_empty(),
        "超限流不得收到任何业务响应，got: {sink:?}"
    );

    // 释放第一条流后配额归还，新流恢复服务
    held.close().await.expect("close held");
    let mut resp = Vec::new();
    held.read_to_end(&mut resp).await.expect("held echo");
    assert_eq!(resp, [1]);

    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut ok = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open after release");
    ok.write_all(b"again").await.expect("write");
    ok.close().await.expect("close");
    let mut resp = Vec::new();
    ok.read_to_end(&mut resp).await.expect("read");
    assert_eq!(resp, [1], "配额释放后新流必须恢复服务");

    router.shutdown().await;
    client.close().await;
    server.close().await;
}
