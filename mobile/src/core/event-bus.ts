import { t } from "@lingui/core/macro";
import type {
  ForeignEventBus as EventBusContract,
  MobileCoreEvent,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
import {
  clearTransferProgress,
  updateTransferProgress,
} from "@/core/foreground-service";
import {
  fireNotifyPairingRequest,
  fireNotifyTransferOffer,
} from "@/core/notifier";
import { isErrorKind } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { useNotificationStore } from "@/stores/notification-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useTransferStore } from "@/stores/transfer-store";

type CoreEventListener = (event: MobileCoreEvent) => void;

const listeners = new Set<CoreEventListener>();

/** ForeignEventBus implementation. `emit` is called from the Rust runtime
 *  thread — MUST return fast, no awaits, no long work. Long-running effects
 *  (like refreshing device lists from the bridge) should be fire-and-forget. */
export class EventBus implements EventBusContract {
  emit(event: MobileCoreEvent): void {
    for (const listener of listeners) {
      try {
        listener(event);
      } catch (err) {
        console.warn("[event-bus] listener threw:", err);
      }
    }
    routeEventToStores(event);
  }
}

function routeEventToStores(event: MobileCoreEvent): void {
  switch (event.tag) {
    case MobileCoreEvent_Tags.NetworkStatusChanged: {
      // status 已经是 ubrn 生成的 MobileNetworkStatus,直接透传
      useMobileCoreStore.getState().applyNetworkStatus(event.inner.status);
      break;
    }

    case MobileCoreEvent_Tags.DevicesChanged: {
      refreshDevices();
      break;
    }

    case MobileCoreEvent_Tags.PairingRequestReceived: {
      const { pendingId, peerId, code } = event.inner;
      useNotificationStore.getState().push({
        id: `pairing-${pendingId.toString()}-${Date.now()}`,
        type: "pairing-request",
        payload: {
          pendingId,
          peerId,
          code: code ?? undefined,
          receivedAt: Date.now(),
        },
        timestamp: Date.now(),
      });
      fireNotifyPairingRequest(peerId, pendingId, code ?? undefined);
      break;
    }

    case MobileCoreEvent_Tags.PairedDeviceAdded: {
      // 两个来源:(1) 配对成功时 mobile-core pairing.rs 主动 publish;
      // (2) 对端经 Identify 广播新设备名时共享 core publish。
      // 两种情况原生都已把设备写回 keychain,这里刷新离线兜底 cache,
      // 使新设备 / 新名称在节点未运行、设备离线或重启后仍展示。
      refreshDevices();
      void useMobileCoreStore.getState().loadPairedDevicesCache();
      break;
    }

    case MobileCoreEvent_Tags.PairedDeviceRemoved: {
      // 唯一触发点是 core 的 unpair,且只在集合真的变了时才发。
      // 与 PairedDeviceAdded 对称:持久化已由 core 写完,这里只把两份读模型收敛回
      // 桥的事实源 —— pairedDevicesCache 里该设备消失,devices 里它退回"仅发现、
      // 未配对"。收敛而非本地删,是为了不依赖「谁发起的解除」:store action
      // removePairedDevice 按命令返回值同步过一次,这里再收敛一次是幂等的。
      refreshDevices();
      void useMobileCoreStore.getState().loadPairedDevicesCache();
      break;
    }

    case MobileCoreEvent_Tags.DeviceRenamed: {
      // 本机改名的广播口 —— 发起改名的那处界面之外(设置页设备卡、onboarding 回显)
      // 都靠它同步,不必各自轮询 core。
      //
      // 取的是 `name` 而不是 core 算好的 `displayName`:移动端 env 探测不到真 hostname,
      // core 侧 OsInfo 的 hostname 是占位串 "Device"(见 mobile-core network.rs),
      // 清空名字时 displayName 会退化成它。空串在 UI 层回退到 expo-device 的设备名,
      // 那才是移动端该显示的东西。
      usePreferencesStore.getState().setDeviceName(event.inner.name ?? "");
      break;
    }

    case MobileCoreEvent_Tags.TransferOfferReceived: {
      // offer 已经是 ubrn 生成的 MobileTransferOffer,store 类型也对齐它,直接透传
      const offer = event.inner.offer;
      useTransferStore.getState().pushOffer(offer);
      fireNotifyTransferOffer(
        offer.sessionId,
        offer.deviceName,
        offer.files.length,
      );
      break;
    }

    case MobileCoreEvent_Tags.TransferProgress: {
      const { progress } = event.inner;
      useTransferStore.getState().updateProgress(progress);
      // 前台服务进度通知(Android;iOS 内部 no-op,进度仅应用内)
      void updateTransferProgress(progress);
      break;
    }

    case MobileCoreEvent_Tags.FilePublish: {
      // 「字节收完」不等于「文件已保存」：数据先落进程私有的暂存区，收齐才发布到用户
      // 可见位置。Android 的 SAF 目标那一段是全量字节拷贝、几十秒起步，而此时进度条
      // 已经满了——没有这条事件，用户看到的就是「满了之后凭空多等一段」。
      //
      // 逐文件发生（收齐即发布），所以一个多文件会话会来很多次，不是末尾一次。
      useTransferStore.getState().applyFilePublish(event.inner.event);
      break;
    }

    case MobileCoreEvent_Tags.PrepareProgress: {
      // 发送准备进度（一遍流式读产出 checksum + 验签树）。落 store 而非页面 useState：
      // 准备大目录可以是分钟级，用户切走再回来得看得到它。
      useTransferStore.getState().updatePrepare(event.inner.event);
      break;
    }

    case MobileCoreEvent_Tags.TransferProjectionUpdate: {
      useTransferStore.getState().applyProjection(event.inner.projection);
      break;
    }

    case MobileCoreEvent_Tags.TransferAccepted: {
      // 状态由 TransferProjectionUpdate 接管，无需额外处理。
      break;
    }

    case MobileCoreEvent_Tags.TransferRejected: {
      // core 已携带拒绝原因(策略拒绝/未配对等),透传出来而非只显示通用文案。
      const { reason } = event.inner;
      useTransferStore
        .getState()
        .setError(
          reason ? t`对方拒绝了传输请求：${reason}` : t`对方拒绝了传输请求`,
        );
      break;
    }

    case MobileCoreEvent_Tags.TransferCompleted: {
      // 传输状态由 TransferProjectionUpdate 接管；这里只刷新收件箱。
      void refreshInbox();
      // 传输结束 → 前台服务通知回到 idle 保活文案
      void clearTransferProgress();
      break;
    }

    case MobileCoreEvent_Tags.TransferFailed: {
      const { error, sessionId } = event.inner;
      // 发布失败不会补发 `finished`，横幅只能靠这里（与 Paused / 非活跃投影）收掉。
      useTransferStore.getState().clearPublishing(sessionId);
      if (error.startsWith("对方取消")) {
        const message = t`对方已取消传输`;
        toast.info(message);
        useTransferStore.getState().setError(message);
      } else {
        toast.error(t`传输失败`, error);
        useTransferStore.getState().setError(t`传输失败：${error}`);
      }
      // 传输失败 / 被取消 → 前台服务通知回到 idle 保活文案
      void clearTransferProgress();
      break;
    }

    case MobileCoreEvent_Tags.TransferPaused: {
      // 对端暂停：状态由 TransferProjectionUpdate 接管，这里只提示。
      useTransferStore.getState().clearPublishing(event.inner.sessionId);
      const message = t`对方已暂停传输`;
      toast.info(message);
      useTransferStore.getState().setError(message);
      break;
    }

    case MobileCoreEvent_Tags.TransferResumed: {
      // 状态由 TransferProjectionUpdate 接管，无需额外处理。
      break;
    }

    case MobileCoreEvent_Tags.TransferDbError: {
      useTransferStore.getState().setError(event.inner.message);
      break;
    }

    case MobileCoreEvent_Tags.PairingCompleted: {
      // 显式不落：配对完成的状态由 PairedDeviceAdded（上面刷新已配对列表）与
      // DevicesChanged 表达，这里再动一次是第二条写路径。
      //
      // 写成显式空 case 而不是让它掉进 default，是为了保住 `default` 分支「真的遇到了
      // 未知事件」这个信号——它和 PrepareProgress 曾一起漏在那里，每次发送都刷成百上千
      // 条 warn，于是这条日志再也不代表任何东西。
      break;
    }

    case MobileCoreEvent_Tags.Error: {
      useMobileCoreStore.getState().setError(event.inner.message);
      break;
    }

    default: {
      console.warn(
        "[event-bus] unhandled event tag",
        (event as { tag: string }).tag,
      );
    }
  }
}

async function refreshDevices(): Promise<void> {
  // 节点未启动直接跳过 —— Rust 端 list_devices 依赖 NetManager,
  // shutdownNode/startNode 切换期间可能收到清理事件,这时调用会抛 NodeNotStarted。
  if (useMobileCoreStore.getState().runtimeState !== "running") return;
  try {
    const { getMobileCore } = await import("./mobile-core");
    const devices = await getMobileCore().listDevices("all");
    useMobileCoreStore.getState().applyDevices(devices);
  } catch (err) {
    // NodeNotStarted 在节点状态切换的窗口期是预期错误,静默忽略。
    // **按 tag 判别，不是按 message 找子串** —— message 是 Rust 侧写的自然语言，
    // 改一个字这条静默就失效，表现为切换节点时冒出一串无害的 warn。
    if (isErrorKind(err, "NodeNotStarted")) return;
    console.warn("[event-bus] listDevices failed:", err);
  }
}

async function refreshInbox(): Promise<void> {
  try {
    const { useInboxStore } = await import("@/stores/inbox-store");
    await useInboxStore.getState().refresh();
  } catch (err) {
    console.warn("[event-bus] refreshInbox failed:", err);
  }
}

export const mobileEventBus = new EventBus();

export function subscribeCoreEvents(listener: CoreEventListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
