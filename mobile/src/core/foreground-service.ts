import { t } from "@lingui/core/macro";
import { calcPercent } from "@swarmdrop/shared-view";
import { Platform } from "react-native";
import notifee, {
  AndroidForegroundServiceType,
  AndroidImportance,
  type Event,
  EventType,
} from "react-native-notify-kit";
import type { MobileTransferProgress } from "react-native-swarmdrop-core";
import { etaText, formatSpeed } from "@/components/transfer/shared";
import { getMobileCore } from "@/core/mobile-core";
import type { TransferDirection } from "@/core/transfer-types";

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
  // 状态栏单色小图标(白色橡子剪影,由 with-android-notification-icon plugin 注入
  // res/drawable-*/ic_notification.png)。不设则 notify-kit 回退到彩色 launcher 图标
  // → 被系统渲染成深色块。color 给图标染品牌绿。
  smallIcon: "ic_notification",
  color: "#0F8F7A",
  asForegroundService: true,
  foregroundServiceTypes: [FGS_TYPE],
  ongoing: true,
  onlyAlertOnce: true,
  pressAction: { id: "default" },
};

let serviceRegistered = false;
let backgroundEventRegistered = false;
let channelReady: Promise<void> | null = null;
let running = false;
/** 当前在途传输 sessionId —— action 事件里 notification.data 缺失时的兜底。 */
let activeSessionId: string | null = null;
/** 当前在途传输方向 —— 与 activeSessionId 同款兜底,暂停/取消要按方向分派到对应导出。 */
let activeDirection: TransferDirection | null = null;
/** 进度刷新限流:上次刷新时间与百分比。 */
let lastProgressAt = 0;
let lastProgressPct = -1;
/**
 * 当前正在发布的文件名(不在发布期为 null)。
 *
 * 只用来判「通知换了个模式」:传输态与发布态共用上面那对限流变量,而换模式时标题整句
 * 都变了,再按百分比去重就会把新标题按下去(发布从 0% 开始,传输恰好停在 100%)。
 */
let publishingName: string | null = null;

/** 回到「没有在途通知」的干净状态。 */
function resetProgressThrottle(): void {
  publishingName = null;
  lastProgressAt = 0;
  lastProgressPct = -1;
}

/** 切换通知模式(传输 ⇄ 发布某个文件)时重置限流,让新标题立刻画出来。 */
function enterNotificationMode(nextPublishingName: string | null): void {
  if (publishingName === nextPublishingName) return;
  publishingName = nextPublishingName;
  lastProgressAt = 0;
  lastProgressPct = -1;
}

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
  // 方向与 sessionId 同源(displayNotification 一并写进 data),缺失时一起回落到在途会话。
  const rawDirection = event.detail.notification?.data?.direction;
  const direction: TransferDirection | null =
    rawDirection === "send" || rawDirection === "receive"
      ? rawDirection
      : activeDirection;
  if (sessionId === null || direction === null) return;
  try {
    const core = getMobileCore();
    if (actionId === FGS_ACTION_PAUSE) {
      await (direction === "send"
        ? core.pauseSend(sessionId)
        : core.pauseReceive(sessionId));
    } else if (actionId === FGS_ACTION_CANCEL) {
      await (direction === "send"
        ? core.cancelSend(sessionId)
        : core.cancelReceive(sessionId));
    }
  } catch (err) {
    console.warn(`[fgs] action ${actionId} failed:`, err);
  }
}

/**
 * app 启动时调用一次:注册前台服务 runner + 后台事件监听。
 * runner 永不 resolve —— 保活由 stopForegroundKeepAlive() 显式拆除。
 * 后台 / 被杀态的 action 必须在此注册,否则丢失。
 *
 * 两个标志分开记,且各自在该步成功之后才置位 —— 理由同 `initNotifications`:
 * 启动失败屏的「重试」会重跑本函数,一个共用标志无论放开头还是结尾都不对
 * (放开头 = 失败后重试整个跳过;放结尾 = 重试把已成功的那步重放,headless task 被重复注册)。
 */
export function initForegroundService(): void {
  if (!isAndroid) return;
  if (!serviceRegistered) {
    notifee.registerForegroundService(() => new Promise<void>(() => {}));
    serviceRegistered = true;
  }
  if (!backgroundEventRegistered) {
    notifee.onBackgroundEvent(handleForegroundServiceEvent);
    backgroundEventRegistered = true;
  }
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
  activeDirection = null;
  resetProgressThrottle();
  try {
    await notifee.stopForegroundService();
  } catch (err) {
    console.warn("[fgs] stop failed:", err);
  }
}

/** 传输 / 发布通知共用的两个动作按钮(会话级,发布期照样可取消)。 */
function transferActions() {
  return [
    { title: t`暂停`, pressAction: { id: FGS_ACTION_PAUSE } },
    { title: t`取消`, pressAction: { id: FGS_ACTION_CANCEL } },
  ];
}

/**
 * 传输 / 发布两条进度通知共用的收尾:护栏 → 模式切换 → 限流 → 展示 → 吞错。
 *
 * 两者的差别只有 title / body / data 三个值,其余十几行逐字相同 —— 分开写的代价是限流
 * 与护栏两次实现,而它们共享 [`lastProgressPct`] 这一对模块变量,漂一处就是「换了模式
 * 却被按百分比去重按住」那类只在特定时序下复现的 bug。
 *
 * `nextMode` 兼作模式判别(`null` = 传输态),见 [`enterNotificationMode`]。
 *
 * **形参刻意不叫 `publishingName`**:那是模块级可变量,存的是「当前是什么模式」,而这里
 * 传进来的是「要切到什么模式」。同名会让后续任何想在本函数里读当前模式的改动静默读到形参,
 * 于是 `enterNotificationMode` 的去重判据在读者心里变成永真 —— 而限流状态是两种模式共用
 * 的那一对模块变量,正是本文件注释点名「漂一处就只在特定时序下复现」的地方。
 */
async function displayProgressNotification(
  nextMode: string | null,
  pct: number,
  content: { title: string; body: string; data: Record<string, string> },
): Promise<void> {
  if (!isAndroid || !running) return;
  enterNotificationMode(nextMode);
  // 限流:进度百分比未变且距上次 < 500ms 则跳过(高频事件抖动 / 省电)。
  const now = Date.now();
  if (pct === lastProgressPct && now - lastProgressAt < 500) return;
  lastProgressAt = now;
  lastProgressPct = pct;
  try {
    await notifee.displayNotification({
      id: FGS_NOTIFICATION_ID,
      title: content.title,
      body: content.body,
      data: content.data,
      android: {
        ...FGS_ANDROID_BASE,
        progress: { max: 100, current: pct },
        actions: transferActions(),
      },
    });
  } catch (err) {
    console.warn("[fgs] progress update failed:", err);
  }
}

/** 传输进度驱动前台服务通知(按同一 id 更新,限流防抖)。仅 FGS 运行时生效。 */
export async function updateTransferProgress(
  p: MobileTransferProgress,
): Promise<void> {
  if (!isAndroid || !running) return;
  const pct = calcPercent(p.transferredBytes, p.totalBytes);
  const direction: TransferDirection =
    p.direction === "send" ? "send" : "receive";
  // 记的是「当前在途的是哪条」,不是「上次画出去的是哪条」——所以每帧都记,不等限流放行。
  // 通知 action 的兜底与发布态通知的 data 都读它们(发布期本层拿不到会话 id)。
  activeSessionId = p.sessionId;
  activeDirection = direction;

  const dirLabel = direction === "send" ? t`发送中` : t`接收中`;
  const fileCount = `${p.completedFiles}/${p.totalFiles}`;
  // 「剩余多久」在这块面上价值最高:用户切走之后这是唯一还能看到进度的地方。
  // 算不出来给占位而不是省掉那一格 —— 契约明写槽位不得消失。
  const etaLabel = etaText(p.eta);
  const speedLabel = formatSpeed(p.speed);
  await displayProgressNotification(null, pct, {
    title: `${dirLabel} · ${pct}%`,
    // **ETA 排在速度前面**:折叠态只有一行、超长从**尾部**省略,顺序反了被截掉的
    // 恰好是 ETA,与契约的「只放得下一个时 ETA 优先」相反。
    body: t`${fileCount} 个文件 · ${etaLabel} · ${speedLabel}`,
    data: { kind: "transfer-progress", sessionId: p.sessionId, direction },
  });
}

/**
 * 发布阶段(暂存 → 用户可见位置)的通知。
 *
 * **只有 Android 的 SAF 目标会走到这里**:那条发布路径是全量字节拷贝(6 GB 的文件要写
 * 12 GB),几十秒起步;其余平台是 O(1) 重命名,没有可展示的过程。此时字节已经收完、
 * 传输进度条早已满格,不换文案的话通知会停在「接收中 · 100%」一动不动 —— 那正是用户
 * 读成「卡死了」进而强杀应用的形状。
 *
 * 调用方是 `ForeignFileAccess` 的拷贝循环,自带 200ms 节流。
 */
export async function updatePublishProgress(
  name: string,
  publishedBytes: number,
  totalBytes: number,
): Promise<void> {
  const pct = calcPercent(publishedBytes, totalBytes);
  await displayProgressNotification(name, pct, {
    // 文件名不进标题:它可能很长,而百分比是这一屏唯一在动的数字,得留在最前面。
    title: `${t`正在保存…`} · ${pct}%`,
    body: name,
    // 会话归属沿用上一帧传输进度写下的 activeSessionId / activeDirection ——
    // 本层拿不到会话 id(拷贝循环只认 relativePath),而点击跳转与两个动作按钮都会
    // 回落到那两个模块变量(见 handleForegroundServiceEvent / notification-router)。
    data:
      activeSessionId && activeDirection
        ? {
            kind: "transfer-progress",
            sessionId: activeSessionId,
            direction: activeDirection,
          }
        : { kind: "transfer-progress" },
  });
}

/** 传输结束(完成 / 失败 / 取消):node 仍运行,回到 idle 保活文案。 */
export async function clearTransferProgress(): Promise<void> {
  if (!isAndroid || !running) return;
  activeSessionId = null;
  activeDirection = null;
  resetProgressThrottle();
  try {
    await displayKeepAlive();
  } catch (err) {
    console.warn("[fgs] clear progress failed:", err);
  }
}
