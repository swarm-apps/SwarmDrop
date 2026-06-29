import { t } from "@lingui/core/macro";
import type {
  ForeignEventBus as EventBusContract,
  MobileCoreEvent,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
import {
  fireNotifyPairingRequest,
  fireNotifyTransferOffer,
} from "@/core/notifier";
import { toast } from "@/lib/toast";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { useNotificationStore } from "@/stores/notification-store";
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
      fireNotifyPairingRequest(peerId, code ?? undefined);
      break;
    }

    case MobileCoreEvent_Tags.PairingCompleted: {
      refreshDevices();
      // 新配对设备已写入 keychain,同步刷新 UI 兜底 cache(不依赖节点状态)
      void useMobileCoreStore.getState().loadPairedDevicesCache();
      break;
    }

    case MobileCoreEvent_Tags.TransferOfferReceived: {
      // offer 已经是 ubrn 生成的 MobileTransferOffer,store 类型也对齐它,直接透传
      const offer = event.inner.offer;
      useTransferStore.getState().pushOffer(offer);
      fireNotifyTransferOffer(offer.deviceName, offer.files.length);
      break;
    }

    case MobileCoreEvent_Tags.TransferProgress: {
      const { progress } = event.inner;
      useTransferStore.getState().updateProgress(progress);
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
      break;
    }

    case MobileCoreEvent_Tags.TransferFailed: {
      const { error } = event.inner;
      if (error.startsWith("对方取消")) {
        const message = t`对方已取消传输`;
        toast.info(message);
        useTransferStore.getState().setError(message);
      } else {
        toast.error(t`传输失败`, error);
        useTransferStore.getState().setError(t`传输失败：${error}`);
      }
      break;
    }

    case MobileCoreEvent_Tags.TransferPaused: {
      // 对端暂停：状态由 TransferProjectionUpdate 接管，这里只提示。
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
    // NodeNotStarted 在节点状态切换的窗口期是预期错误,静默忽略
    const msg = err instanceof Error ? err.message : String(err);
    if (msg.includes("NodeNotStarted")) return;
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
