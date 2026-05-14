import type {
  ForeignEventBus as EventBusContract,
  MobileCoreEvent,
} from "react-native-swarmdrop-core";
import { MobileCoreEvent_Tags } from "react-native-swarmdrop-core";
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
      break;
    }

    case MobileCoreEvent_Tags.PairingCompleted: {
      refreshDevices();
      break;
    }

    case MobileCoreEvent_Tags.TransferOfferReceived: {
      // offer 已经是 ubrn 生成的 MobileTransferOffer,store 类型也对齐它,直接透传
      useTransferStore.getState().pushOffer(event.inner.offer);
      break;
    }

    case MobileCoreEvent_Tags.TransferProgress: {
      const { sessionId, progress } = event.inner;
      useTransferStore.getState().setProgress(sessionId, progress);
      break;
    }

    case MobileCoreEvent_Tags.TransferCompleted: {
      const { sessionId } = event.inner;
      useTransferStore.getState().setProgress(sessionId, 1);
      break;
    }

    case MobileCoreEvent_Tags.TransferFailed: {
      const { sessionId, error } = event.inner;
      useTransferStore.getState().setError(`传输失败：${error}`);
      useTransferStore.getState().removeSession(sessionId);
      break;
    }

    case MobileCoreEvent_Tags.TransferPaused: {
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
  try {
    const { getMobileCore } = await import("./mobile-core");
    const devices = await getMobileCore().listDevices("all");
    useMobileCoreStore.getState().applyDevices(devices);
  } catch (err) {
    console.warn("[event-bus] listDevices failed:", err);
  }
}

export const mobileEventBus = new EventBus();

export function subscribeCoreEvents(listener: CoreEventListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
