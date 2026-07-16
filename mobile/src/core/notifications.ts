import notifee from "react-native-notify-kit";
import {
  handleForegroundServiceEvent,
  initForegroundService,
} from "@/core/foreground-service";
import {
  handleForegroundNotificationEvent,
  handleInitialNotification,
} from "@/core/notification-router";

let initialized = false;

/**
 * app 启动时调用一次(_layout boot):
 * - 注册 Android 前台服务 runner + 后台 action 事件监听(在 initForegroundService 内)
 * - 前台通知事件:ACTION_PRESS → 暂停 / 取消;PRESS → 深链跳转
 * - 冷启动:处理拉起 app 的初始通知
 */
export function initNotifications(): void {
  if (initialized) return;
  initialized = true;

  initForegroundService();

  notifee.onForegroundEvent((event) => {
    void handleForegroundServiceEvent(event);
    handleForegroundNotificationEvent(event);
  });

  void handleInitialNotification();
}
