//! 脚本化后端，用于把信令状态机与真实 WebRTC 栈解耦测试。
//!
//! 独立成文件而非塞在 `mod.rs` 的 `#[cfg(test)]` 块里——测试替身与生产代码同文件时，
//! 读 trait 定义要先翻过几十行 mock。

//! 脚本化后端，用于把信令状态机与真实 WebRTC 栈解耦测试。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use super::{Backend, BackendError, BackendEvent};

/// 记录状态机对后端的调用，供断言「该调的调了、不该调的没调」。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Calls {
    pub started_offer: bool,
    pub accepted_offer: Vec<String>,
    pub accepted_answer: Vec<String>,
    pub remote_candidates: Vec<String>,
}

pub(crate) struct MockBackend {
    /// 按序吐出的事件；空了就 Pending。
    pub queued: VecDeque<BackendEvent>,
    pub calls: Arc<Mutex<Calls>>,
    /// 令某个调用失败，用于测错误路径。
    pub fail_on_offer: bool,
}

impl MockBackend {
    pub(crate) fn new(events: impl IntoIterator<Item = BackendEvent>) -> (Self, Arc<Mutex<Calls>>) {
        let calls = Arc::new(Mutex::new(Calls::default()));
        (
            Self {
                queued: events.into_iter().collect(),
                calls: calls.clone(),
                fail_on_offer: false,
            },
            calls,
        )
    }
}

impl Backend for MockBackend {
    fn start_offer(&mut self) -> Result<(), BackendError> {
        if self.fail_on_offer {
            return Err(BackendError::new("mock: offer 失败"));
        }
        self.calls.lock().unwrap().started_offer = true;
        Ok(())
    }

    fn accept_offer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.calls.lock().unwrap().accepted_offer.push(sdp.into());
        Ok(())
    }

    fn accept_answer(&mut self, sdp: &str) -> Result<(), BackendError> {
        self.calls.lock().unwrap().accepted_answer.push(sdp.into());
        Ok(())
    }

    fn add_remote_candidate(&mut self, json: &str) -> Result<(), BackendError> {
        self.calls
            .lock()
            .unwrap()
            .remote_candidates
            .push(json.into());
        Ok(())
    }

    fn poll(&mut self, _cx: &mut Context<'_>) -> Poll<BackendEvent> {
        match self.queued.pop_front() {
            Some(e) => Poll::Ready(e),
            None => Poll::Pending,
        }
    }
}
