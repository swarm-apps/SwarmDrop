//! 系统通知的焦点门控与可替换发布边界。
//!
//! 宿主先把语言中立的领域通知本地化为 [`SystemNotification`]，再经本模块决定是否发布。
//! 这样 Tauri 的窗口查询和原生 API 都停留在边缘，而「前台不打扰」可在不加载桌面运行时的
//! 平台中立测试中验证。

use async_trait::async_trait;

use crate::AppResult;

/// 已完成本地化、可交给系统通知 API 的消息。
///
/// 该类型只存在于宿主展示边缘；文本投递的正文不得从领域通知进入这里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemNotification {
    pub title: String,
    pub body: String,
}

/// 原生系统通知的最小副作用端口。
#[async_trait]
pub trait SystemNotificationPublisher: Send + Sync {
    async fn show(&self, notification: SystemNotification) -> AppResult<()>;
}

/// 仅在没有聚焦窗口时发布系统通知。
pub async fn publish_if_window_unfocused(
    has_focused_window: bool,
    notification: SystemNotification,
    publisher: &dyn SystemNotificationPublisher,
) -> AppResult<()> {
    if has_focused_window {
        return Ok(());
    }
    publisher.show(notification).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::{SystemNotification, SystemNotificationPublisher, publish_if_window_unfocused};
    use crate::{AppError, AppResult};

    #[derive(Default)]
    struct FakePublisher {
        notifications: Mutex<Vec<SystemNotification>>,
        fail: bool,
    }

    #[async_trait]
    impl SystemNotificationPublisher for FakePublisher {
        async fn show(&self, notification: SystemNotification) -> AppResult<()> {
            self.notifications
                .lock()
                .expect("记录通知")
                .push(notification);
            if self.fail {
                return Err(AppError::Network("模拟系统通知失败".into()));
            }
            Ok(())
        }
    }

    fn incoming_text_notification() -> SystemNotification {
        SystemNotification {
            title: "收到文本".into(),
            body: "Alice 向你发送了一段文本".into(),
        }
    }

    #[tokio::test]
    async fn focused_window_suppresses_system_notification() {
        let publisher = FakePublisher::default();

        publish_if_window_unfocused(true, incoming_text_notification(), &publisher)
            .await
            .expect("前台抑制不是错误");

        assert!(
            publisher.notifications.lock().expect("读取通知").is_empty(),
            "前台窗口不得创建系统通知"
        );
    }

    #[tokio::test]
    async fn unfocused_window_publishes_the_generic_text_notification() {
        let publisher = FakePublisher::default();
        let sensitive_body = "不能出现在系统通知中的敏感正文";

        publish_if_window_unfocused(false, incoming_text_notification(), &publisher)
            .await
            .expect("后台窗口应发送通知");

        let notifications = publisher.notifications.lock().expect("读取通知");
        assert_eq!(notifications.len(), 1, "后台窗口应只发送一次通知");
        assert!(
            !notifications[0].body.contains(sensitive_body),
            "文本正文不得泄露到锁屏或系统历史"
        );
    }

    #[tokio::test]
    async fn failed_native_notification_is_reported_after_a_single_attempt() {
        let publisher = FakePublisher {
            fail: true,
            ..Default::default()
        };

        let result =
            publish_if_window_unfocused(false, incoming_text_notification(), &publisher).await;

        assert!(result.is_err(), "发布器失败必须由调用方观察到");
        assert_eq!(
            publisher.notifications.lock().expect("读取通知").len(),
            1,
            "通知失败不得在门控层重试或改变投递状态"
        );
    }
}
