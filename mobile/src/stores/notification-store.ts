import { create } from "zustand";

export interface PairingRequestPayload {
  pendingId: bigint;
  peerId: string;
  code?: string;
  receivedAt: number;
}

export interface PairingRequestNotification {
  id: string;
  type: "pairing-request";
  payload: PairingRequestPayload;
  timestamp: number;
}

export type ActionNotification = PairingRequestNotification;

interface NotificationState {
  current: ActionNotification | null;
  queue: ActionNotification[];
}

interface NotificationActions {
  push(notification: ActionNotification): void;
  respond(id: string): void;
  dismiss(id: string): void;
}

export const useNotificationStore = create<
  NotificationState & NotificationActions
>()((set, get) => ({
  current: null,
  queue: [],
  push(notification) {
    const { current } = get();
    if (current === null) {
      set({ current: notification });
    } else {
      set((s) => ({ queue: [...s.queue, notification] }));
    }
  },
  respond(id) {
    const { current, queue } = get();
    if (current?.id === id) {
      const [next, ...rest] = queue;
      set({ current: next ?? null, queue: rest });
    }
  },
  dismiss(id) {
    get().respond(id);
  },
}));
