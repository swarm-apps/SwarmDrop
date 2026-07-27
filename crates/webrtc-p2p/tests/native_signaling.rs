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
