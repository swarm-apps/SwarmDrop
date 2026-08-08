import notifee from "react-native-notify-kit";
import {
  handleForegroundServiceEvent,
  initForegroundService,
} from "@/core/foreground-service";
import {
  handleForegroundNotificationEvent,
  handleInitialNotification,
} from "@/core/notification-router";

let foregroundEventRegistered = false;
let initialNotificationHandled = false;

/**
 * app 启动时调用一次(_layout boot):
 * - 注册 Android 前台服务 runner + 后台 action 事件监听(在 initForegroundService 内)
 * - 前台通知事件:ACTION_PRESS → 暂停 / 取消;PRESS → 深链跳转
 * - 冷启动:处理拉起 app 的初始通知
 *
 * **每一步各自记标志,且标志都在该步成功之后才置位。** 启动失败屏的「重试」会重跑本函数,
 * 所以它必须能安全重入:
 * - 一个总的 `initialized` 进门就置位 → 任一步抛出后重试直接 return,通知系统整个没注册,
 *   而 App 看起来是正常起来的(长传在后台被杀、点通知没反应,且无处报错);
 * - 一个总的 `initialized` 放在末尾置位 → 重试会把**已经成功的那几步重放一遍**,
 *   于是 headless task 与事件监听被重复注册。
 * 分步记录同时排除这两种。
 */
export function initNotifications(): void {
  initForegroundService();

  if (!foregroundEventRegistered) {
    notifee.onForegroundEvent((event) => {
      void handleForegroundServiceEvent(event);
      handleForegroundNotificationEvent(event);
    });
    foregroundEventRegistered = true;
  }

  if (!initialNotificationHandled) {
    void handleInitialNotification();
    initialNotificationHandled = true;
  }
}
