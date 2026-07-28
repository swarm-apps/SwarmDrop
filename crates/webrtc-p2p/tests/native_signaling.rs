//! 两个真实 `webrtc-rs` 后端跑通信令全链路。
//!
//! 这是「打洞方案成立」在本地能验到的最强证据：不 mock 任何东西，走完 spec 步骤 4–8
//! （init 通道 → offer → answer → trickle ICE → Connected），只把信令消息在两端之间
//! 手工搬运——那正是真实场景里 relay 承担的角色。
//!
//! 单测里的 mock 后端验的是「状态机在什么时候该做什么」，这里验的是「照那样做真能连上」。

#![cfg(not(target_family = "wasm"))]

use std::task::Poll;
use std::time::Duration;

use futures::FutureExt;
use webrtc_p2p::backend::native::NativeBackend;
use webrtc_p2p::{Backend, BackendEvent, Config, MessageType};

/// 非阻塞地取一个后端事件。
///
/// 用轮询而非 waker 驱动：测试要同时推进两个后端并在它们之间搬运消息，轮询的控制流
/// 直白得多，代价只是多睡几毫秒。
fn try_next(backend: &mut dyn Backend) -> Option<BackendEvent> {
    match std::future::poll_fn(|cx| Poll::Ready(backend.poll(cx))).now_or_never() {
        Some(Poll::Ready(event)) => Some(event),
        _ => None,
    }
}

/// 把 `from` 产出的信令投给 `to`，返回 `from` 是否已连通。
fn relay_one(from: &mut dyn Backend, to: &mut dyn Backend) -> Result<bool, String> {
    let Some(event) = try_next(from) else {
        return Ok(false);
    };
    match event {
        BackendEvent::LocalDescription { ty, sdp } => {
            match ty {
                MessageType::SdpOffer => to.accept_offer(&sdp),
                MessageType::SdpAnswer => to.accept_answer(&sdp),
                MessageType::IceCandidate => unreachable!("SDP 不会是候选类型"),
            }
            .map_err(|e| e.to_string())?;
            Ok(false)
        }
        BackendEvent::LocalCandidate(json) => {
            to.add_remote_candidate(&json).map_err(|e| e.to_string())?;
            Ok(false)
        }
        BackendEvent::Connected => Ok(true),
        BackendEvent::Failed(e) => Err(e),
    }
}

#[tokio::test]
async fn two_native_backends_complete_signaling() {
    let config = Config::default();
    let mut initiator = NativeBackend::new(&config);
    let mut responder = NativeBackend::new(&config);

    // spec 步骤 4：发起方建 init 通道并造 offer。
    initiator.start_offer().unwrap();

    let (mut a_up, mut b_up) = (false, false);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    while !(a_up && b_up) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "30s 内未完成信令：initiator_connected={a_up} responder_connected={b_up}"
        );

        a_up |= relay_one(&mut initiator, &mut responder).expect("发起方信令失败");
        b_up |= relay_one(&mut responder, &mut initiator).expect("应答方信令失败");

        // 让 webrtc-rs 的内部任务有机会推进（ICE 检查、DTLS 握手都在那边跑）。
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// 建连之后真的能收发数据。
///
/// 这是数据面的决定性验证：信令只证明「握上手了」，这里证明「握完能说话」——
/// DataChannel → 字节流 → libp2p framing 整条适配链都得对，错一环这个测试就挂。
#[tokio::test]
async fn established_connection_carries_data() {
    use futures::{AsyncReadExt, AsyncWriteExt};
    use libp2p_core::muxing::StreamMuxer;
    use std::pin::Pin;

    let config = Config::default();
    let mut initiator = NativeBackend::new(&config);
    let mut responder = NativeBackend::new(&config);
    initiator.start_offer().unwrap();

    let (mut a_up, mut b_up) = (false, false);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !(a_up && b_up) {
        assert!(tokio::time::Instant::now() < deadline, "30s 内未完成信令");
        a_up |= relay_one(&mut initiator, &mut responder).expect("发起方信令失败");
        b_up |= relay_one(&mut responder, &mut initiator).expect("应答方信令失败");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let mut muxer_a = initiator.take_muxer().expect("建连后应能取到数据面");
    let mut muxer_b = responder.take_muxer().expect("建连后应能取到数据面");
    assert!(
        initiator.take_muxer().is_none(),
        "数据面只能取一次，所有权已交出"
    );

    // 一端开流、另一端收流；两个方向必须并发驱动，否则互相等待。
    let (outbound, inbound) = tokio::time::timeout(
        Duration::from_secs(15),
        futures::future::join(
            std::future::poll_fn(|cx| Pin::new(&mut muxer_a).poll_outbound(cx)),
            std::future::poll_fn(|cx| Pin::new(&mut muxer_b).poll_inbound(cx)),
        ),
    )
    .await
    .expect("15s 内应完成开流");

    let mut sender = outbound.expect("开出站流失败");
    let mut receiver = inbound.expect("收入站流失败");

    const MSG: &[u8] = b"hello over webrtc datachannel";
    sender.write_all(MSG).await.expect("写入失败");
    sender.flush().await.expect("flush 失败");

    let mut buf = vec![0u8; MSG.len()];
    tokio::time::timeout(Duration::from_secs(15), receiver.read_exact(&mut buf))
        .await
        .expect("15s 内应收到数据")
        .expect("读取失败");
    assert_eq!(buf, MSG, "收到的字节应与发出的一致");
}

/// 对端从不回应时，信令必须超时收场。
///
/// 没有它，一个沉默的对端会把会话连同其所在的 relay 连接一起永久占住——handler 的
/// keep-alive 是「会话未结束就保持」，而会话永远不结束。
///
/// 放在集成测试而非单测：计时器到点要靠运行时唤醒 waker，单测的 noop waker 叫不醒。
#[tokio::test]
async fn signaling_times_out_when_peer_never_answers() {
    use webrtc_p2p::error::Error;
    use webrtc_p2p::swarm::session::{Action, Role, Session};

    let config = Config::default().with_signaling_timeout(Duration::from_millis(200));
    let mut session = Session::new(config, NativeBackend::factory());
    // 装上流但不喂任何消息——模拟对端收下 offer 后石沉大海。
    session.attach_stream(Role::Responder);

    let action = tokio::time::timeout(
        Duration::from_secs(5),
        std::future::poll_fn(|cx| session.poll(cx)),
    )
    .await
    .expect("应在超时时限内自行收场");

    assert!(
        matches!(action, Action::Failed(Error::SignalingTimeout)),
        "应报超时，实得 {action:?}"
    );
    assert!(session.is_finished());
}
