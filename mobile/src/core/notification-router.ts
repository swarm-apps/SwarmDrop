import { router } from "expo-router";
import notifee, { type Event, EventType } from "react-native-notify-kit";

type NotificationData = { [key: string]: string | object | number } | undefined;

/**
 * 通知点击 → 应用内导航。
 * - transfer-progress:进 /transfer/[sessionId]。
 * - 配对请求 / 传输 offer:全局 host(PairingRequestHost / TransferOfferHost)由
 *   notification-store / transfer-store 驱动,点击只需把 app 带到前台,host 自会弹出。
 */
function routeFromData(data: NotificationData): void {
  if (!data) return;
  const kind = typeof data.kind === "string" ? data.kind : undefined;
  const sessionId =
    typeof data.sessionId === "string" ? data.sessionId : undefined;
  if (kind === "transfer-progress" && sessionId) {
    router.navigate(`/transfer/${sessionId}` as never);
  }
}

/** 前台通知事件:PRESS 深链跳转(ACTION_PRESS 由 foreground-service 处理)。 */
export function handleForegroundNotificationEvent(event: Event): void {
  if (event.type === EventType.PRESS) {
    routeFromData(event.detail.notification?.data);
  }
}

/** 冷启动:app 被点击通知拉起时处理一次初始通知。 */
export async function handleInitialNotification(): Promise<void> {
  const initial = await notifee.getInitialNotification();
  if (initial) {
    routeFromData(initial.notification.data);
  }
}
