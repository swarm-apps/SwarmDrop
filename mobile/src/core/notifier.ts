import { t } from "@lingui/core/macro";
import { AppState } from "react-native";
import notifee, {
  AndroidImportance,
  AuthorizationStatus,
} from "react-native-notify-kit";

/**
 * 配对 / 传输告警的专用高优先级渠道(heads-up 抬头)。
 * 前台服务保活 / 传输进度通知使用独立渠道(foreground-service.ts),不复用此渠道。
 */
const ALERT_CHANNEL_ID = "pairing-transfer-alerts";

let channelReady: Promise<void> | null = null;

/** 幂等创建 Android 告警渠道;iOS 无渠道概念,createChannel 直接 resolve。 */
function ensureAlertChannel(): Promise<void> {
  if (channelReady === null) {
    channelReady = notifee
      .createChannel({
        id: ALERT_CHANNEL_ID,
        name: t`配对与传输请求`,
        importance: AndroidImportance.HIGH,
      })
      .then(() => undefined)
      .catch((err) => {
        console.warn("[notifier] createChannel failed:", err);
        channelReady = null; // 失败后允许下次重试
      });
  }
  return channelReady;
}

export async function ensureNotificationPermission(): Promise<boolean> {
  const settings = await notifee.requestPermission();
  return isAuthorized(settings.authorizationStatus);
}

/**
 * 只检查、不请求 —— 事件驱动的通知发送路径用这个:app 在后台时没有 Activity,
 * requestPermission 无法弹授权窗,会静默失败。请求权限的时机在前台
 * (onboarding / 节点启动预热 / 设置页手动)。
 */
async function hasNotificationPermission(): Promise<boolean> {
  const settings = await notifee.getNotificationSettings();
  return isAuthorized(settings.authorizationStatus);
}

function isAuthorized(status: AuthorizationStatus): boolean {
  return (
    status === AuthorizationStatus.AUTHORIZED ||
    status === AuthorizationStatus.PROVISIONAL
  );
}

/**
 * 与桌面端同一策略:app 在前台时不发系统通知 —— 应用内已有 offer 弹窗 /
 * 通知中心承接;只有退到后台(或被覆盖)才用系统通知把用户拉回来。
 */
function isAppInForeground(): boolean {
  return AppState.currentState === "active";
}

/** 权限被拒后的回退:跳系统通知设置让用户手动开启。 */
export function openNotificationSettings(): Promise<void> {
  return notifee.openNotificationSettings();
}

async function notifyTransferOffer(
  sessionId: string,
  deviceName: string,
  fileCount: number,
): Promise<void> {
  if (isAppInForeground()) return;
  if (!(await hasNotificationPermission())) {
    return;
  }
  await ensureAlertChannel();
  await notifee.displayNotification({
    title: t`收到文件传输请求`,
    body: t`${deviceName} 想发送 ${fileCount} 个文件`,
    data: { kind: "transfer-offer", sessionId },
    android: {
      channelId: ALERT_CHANNEL_ID,
      // 状态栏单色小图标(见 foreground-service.ts 说明);color 染品牌绿。
      smallIcon: "ic_notification",
      color: "#0F8F7A",
      pressAction: { id: "default" },
    },
  });
}

async function notifyPairingRequest(
  peerId: string,
  pendingId: bigint,
  code: string | undefined,
): Promise<void> {
  if (isAppInForeground()) return;
  if (!(await hasNotificationPermission())) {
    return;
  }
  await ensureAlertChannel();

  // 配对时还不知道对方 hostname(要等用户接受后才进 device 列表),
  // 用 PeerId 前 10 位 + 配对码做副标题。
  const peerHint = peerId.slice(0, 10);
  const codeHint = code ? t`配对码 ${code}` : t`扫码配对`;
  await notifee.displayNotification({
    title: t`新设备请求配对`,
    body: `${peerHint}... · ${codeHint}`,
    data: { kind: "pairing-request", pendingId: pendingId.toString(), peerId },
    android: {
      channelId: ALERT_CHANNEL_ID,
      // 状态栏单色小图标(见 foreground-service.ts 说明);color 染品牌绿。
      smallIcon: "ic_notification",
      color: "#0F8F7A",
      pressAction: { id: "default" },
    },
  });
}

/**
 * Fire-and-forget 包装:EventBus.emit 来自 Rust 线程,不能 await,
 * 把 notification 投递异常吞掉避免影响事件分发。
 */
export function fireNotifyTransferOffer(
  sessionId: string,
  deviceName: string,
  fileCount: number,
): void {
  notifyTransferOffer(sessionId, deviceName, fileCount).catch((err) => {
    console.warn("[notifier] notifyTransferOffer failed:", err);
  });
}

export function fireNotifyPairingRequest(
  peerId: string,
  pendingId: bigint,
  code: string | undefined,
): void {
  notifyPairingRequest(peerId, pendingId, code).catch((err) => {
    console.warn("[notifier] notifyPairingRequest failed:", err);
  });
}
