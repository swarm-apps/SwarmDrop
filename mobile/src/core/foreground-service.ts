import { t } from "@lingui/core/macro";
import { Platform } from "react-native";
import notifee, {
  AndroidForegroundServiceType,
  AndroidImportance,
  type Event,
  EventType,
} from "react-native-notify-kit";
import type { MobileTransferProgress } from "react-native-swarmdrop-core";
import { formatSpeed } from "@/components/transfer/shared";
import { getMobileCore } from "@/core/mobile-core";

/**
 * Android 前台服务:仅负责“举保活票 + 常驻通知”,让 node(tokio+libp2p)在 app
 * 退后台 / 息屏后继续运行、对端可寻址;同一条通知在 active transfer 期间承载进度。
 * FGS 是进程级构造,进程保活即让 native 线程继续跑,JS runner 空转即可。
 * iOS 无此能力(见 mobile-background-keepalive spec 平台边界),全部 no-op。
 */

const isAndroid = Platform.OS === "android";

/** 保活 + 传输进度共用的常驻渠道(低优先级,不抢注意力)。告警走 notifier.ts 的独立高优先级渠道。 */
const KEEPALIVE_CHANNEL_ID = "node-keepalive";
/** 前台服务通知固定 id —— 进度更新按同一 id 覆盖,避免服务重启。 */
const FGS_NOTIFICATION_ID = "swarmdrop-foreground-service";

/** 通知 action id。 */
const FGS_ACTION_PAUSE = "transfer-pause";
const FGS_ACTION_CANCEL = "transfer-cancel";

/** connectedDevice 规避 Android 15 对 dataSync 的 ~6h/24h 时长上限(见 design D8)。 */
const FGS_TYPE =
  AndroidForegroundServiceType.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE;

/** idle 保活与传输进度两条通知共享的 Android 前台服务字段。 */
const FGS_ANDROID_BASE = {
  channelId: KEEPALIVE_CHANNEL_ID,
  asForegroundService: true,
  foregroundServiceTypes: [FGS_TYPE],
  ongoing: true,
  onlyAlertOnce: true,
  pressAction: { id: "default" },
};

let registered = false;
let channelReady: Promise<void> | null = null;
let running = false;
/** 当前在途传输 sessionId —— action 事件里 notification.data 缺失时的兜底。 */
let activeSessionId: string | null = null;
/** 进度刷新限流:上次刷新时间与百分比。 */
let lastProgressAt = 0;
let lastProgressPct = -1;

function ensureKeepAliveChannel(): Promise<void> {
  if (channelReady === null) {
    channelReady = notifee
      .createChannel({
        id: KEEPALIVE_CHANNEL_ID,
        name: t`后台保活与传输`,
        importance: AndroidImportance.LOW,
      })
      .then(() => undefined)
      .catch((err) => {
        console.warn("[fgs] createChannel failed:", err);
        channelReady = null;
      });
  }
  return channelReady;
}

/** 通知 action(暂停 / 取消)路由回 transfer manager —— 前后台共用。 */
export async function handleForegroundServiceEvent(
  event: Event,
): Promise<void> {
  if (event.type !== EventType.ACTION_PRESS) return;
  const actionId = event.detail.pressAction?.id;
  const rawSession = event.detail.notification?.data?.sessionId;
  const sessionId =
    typeof rawSession === "string" ? rawSession : (activeSessionId ?? null);
  if (sessionId === null) return;
  try {
    if (actionId === FGS_ACTION_PAUSE) {
      await getMobileCore().pauseTransfer(sessionId);
    } else if (actionId === FGS_ACTION_CANCEL) {
      await getMobileCore().cancelTransfer(sessionId);
    }
  } catch (err) {
    console.warn(`[fgs] action ${actionId} failed:`, err);
  }
}

/**
 * app 启动时调用一次:注册前台服务 runner + 后台事件监听。
 * runner 永不 resolve —— 保活由 stopForegroundKeepAlive() 显式拆除。
 * 后台 / 被杀态的 action 必须在此注册,否则丢失。
 */
export function initForegroundService(): void {
  if (!isAndroid || registered) return;
  registered = true;
  notifee.registerForegroundService(() => new Promise<void>(() => {}));
  notifee.onBackgroundEvent(handleForegroundServiceEvent);
}

/** 展示 idle 保活通知(node 运行、无在途传输)。start 与传输结束后复用。 */
async function displayKeepAlive(): Promise<void> {
  await ensureKeepAliveChannel();
  await notifee.displayNotification({
    id: FGS_NOTIFICATION_ID,
    title: t`SwarmDrop 正在后台运行`,
    body: t`保持在线以接收配对与文件`,
    android: { ...FGS_ANDROID_BASE },
  });
}

/** 节点启动后拉起前台服务(node running ⇔ FGS up)。幂等。 */
export async function startForegroundKeepAlive(): Promise<void> {
  if (!isAndroid || running) return;
  running = true;
  try {
    await displayKeepAlive();
  } catch (err) {
    running = false;
    console.warn("[fgs] start failed:", err);
  }
}

/** 节点停止时拆除前台服务,移除常驻通知。幂等。 */
export async function stopForegroundKeepAlive(): Promise<void> {
  if (!isAndroid || !running) return;
  running = false;
  activeSessionId = null;
  lastProgressAt = 0;
  lastProgressPct = -1;
  try {
    await notifee.stopForegroundService();
  } catch (err) {
    console.warn("[fgs] stop failed:", err);
  }
}

/** 传输进度驱动前台服务通知(按同一 id 更新,限流防抖)。仅 FGS 运行时生效。 */
export async function updateTransferProgress(
  p: MobileTransferProgress,
): Promise<void> {
  if (!isAndroid || !running) return;
  const totalBytes = Number(p.totalBytes);
  const pct =
    totalBytes > 0
      ? Math.min(
          100,
          Math.round((Number(p.transferredBytes) / totalBytes) * 100),
        )
      : 0;
  // 限流:进度百分比未变且距上次 < 500ms 则跳过(高频事件抖动 / 省电)。
  const now = Date.now();
  if (pct === lastProgressPct && now - lastProgressAt < 500) return;
  lastProgressAt = now;
  lastProgressPct = pct;
  activeSessionId = p.sessionId;

  const dirLabel = p.direction === "send" ? t`发送中` : t`接收中`;
  const fileCount = `${p.completedFiles}/${p.totalFiles}`;
  try {
    await notifee.displayNotification({
      id: FGS_NOTIFICATION_ID,
      title: `${dirLabel} · ${pct}%`,
      body: t`${fileCount} 个文件 · ${formatSpeed(p.speed)}`,
      data: { kind: "transfer-progress", sessionId: p.sessionId },
      android: {
        ...FGS_ANDROID_BASE,
        progress: { max: 100, current: pct },
        actions: [
          { title: t`暂停`, pressAction: { id: FGS_ACTION_PAUSE } },
          { title: t`取消`, pressAction: { id: FGS_ACTION_CANCEL } },
        ],
      },
    });
  } catch (err) {
    console.warn("[fgs] progress update failed:", err);
  }
}

/** 传输结束(完成 / 失败 / 取消):node 仍运行,回到 idle 保活文案。 */
export async function clearTransferProgress(): Promise<void> {
  if (!isAndroid || !running) return;
  activeSessionId = null;
  lastProgressAt = 0;
  lastProgressPct = -1;
  try {
    await displayKeepAlive();
  } catch (err) {
    console.warn("[fgs] clear progress failed:", err);
  }
}
