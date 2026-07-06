/**
 * Pairing Store
 * 管理配对流程的状态机
 */

import { create } from "zustand";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import {
  commands,
  type DeviceInfo,
  type PairingCodeInfo,
  type PairingRefuseReason,
  type PairingRequestPayload,
  type PairingResponse,
} from "@/lib/bindings";
import type { PeerId } from "@/lib/types";
import { isErrorKind, getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import {
  findNetworkDeviceSnapshot,
  startNetworkFromStore,
} from "@/stores/network-store";

export type { PairingRequestPayload };

/** 请求超时时间（毫秒） */
const REQUEST_TIMEOUT_MS = 30_000;

/** 搜索超时时间（毫秒） */
const SEARCH_TIMEOUT_MS = 15_000;

/** 搜索请求版本号，用于取消过期搜索 */
let searchVersion = 0;

/** 带超时的 Promise 包装 */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(t`${label}超时（${ms / 1000}s）`)), ms),
    ),
  ]);
}

/** 检查是否为 NodeNotStarted 错误，如果是则弹出启动提示并返回 true */
function handleNodeNotStarted(err: unknown): boolean {
  if (!isErrorKind(err, "NodeNotStarted")) return false;
  toast.error(t`节点未启动`, {
    description: t`请先启动网络节点`,
    action: {
      label: t`启动`,
      onClick: () => {
        void startNetworkFromStore();
      },
    },
  });
  return true;
}

/** 将 PairingRefuseReason 转换为可展示的本地化消息 */
function getPairingRefuseMessage(reason: PairingRefuseReason): string {
  switch (reason.type) {
    case "user_rejected":
      return t`对方拒绝了配对请求`;
  }
}

/** 配对流程阶段（仅管理出站配对流程） */
export type PairingPhase =
  | { phase: "idle" }
  | { phase: "generating"; codeInfo: PairingCodeInfo }
  | { phase: "inputting" }
  | { phase: "searching"; code: string }
  | { phase: "found"; code: string; deviceInfo: DeviceInfo }
  | { phase: "requesting"; peerId: string }
  | { phase: "success"; peerId: string; deviceName: string }
  | { phase: "error"; message: string };

interface PairingState {
  /** 当前出站配对阶段 */
  current: PairingPhase;
  /**
   * 活跃配对码 —— 与 `current` 解耦，跨页面/弹窗持久化。
   *
   * 生命周期：generateCode → activeCode = codeInfo + 启动自动刷新 timer →
   * 过期或被消耗（PAIRED_DEVICE_ADDED 事件）→ 自动 regenerate；节点停止 / 用户
   * 主动 clearActiveCode → null。UI 仅消费此字段，不要再看 `current.phase`。
   */
  activeCode: PairingCodeInfo | null;
  /** 生成配对码时的错误（瞬时；下一次 generate 清空） */
  codeError: string | null;
  /** 当前展示的入站配对请求（独立于出站流程） */
  incomingRequest: PairingRequestPayload | null;
  /** 入站请求队列（当前已有入站请求展示时排队） */
  inboundQueue: PairingRequestPayload[];

  // === Actions ===

  /** 确保活跃配对码存在（已有则 no-op；否则 generate） */
  ensureActiveCode: () => Promise<void>;
  /** 生成新配对码（强制覆盖现有） */
  generateCode: () => Promise<void>;
  /** 重新生成配对码 */
  regenerateCode: () => Promise<void>;
  /** 清空活跃码（节点停止 / 用户主动 dismiss） */
  clearActiveCode: () => void;
  /** 切换到输入配对码状态 */
  openInput: () => void;
  /** 提交配对码查找设备 */
  searchDevice: (code: string) => Promise<void>;
  /** 发起配对请求（Code 模式） */
  sendPairingRequest: () => Promise<void>;
  /** 处理收到的入站配对请求 */
  handleInboundRequest: (payload: PairingRequestPayload) => void;
  /** 接受配对请求，返回是否成功 */
  acceptRequest: () => Promise<boolean>;
  /** 拒绝配对请求 */
  rejectRequest: () => Promise<void>;
  /** Direct 模式配对（附近设备直连） */
  directPairing: (peerId: PeerId) => Promise<void>;
  /** 处理队列中的下一个入站请求 */
  processNextInbound: () => void;
  /** 重置为 idle 状态 */
  reset: () => void;
}

/** 活跃码自动刷新 timer —— module-level（与 store 实例共生死） */
let autoRefreshTimer: ReturnType<typeof setTimeout> | null = null;

function clearAutoRefreshTimer() {
  if (autoRefreshTimer !== null) {
    clearTimeout(autoRefreshTimer);
    autoRefreshTimer = null;
  }
}

/** 调度过期前的自动重生（提前 500ms 拿新码避免 UI 闪 expired） */
function scheduleAutoRefresh(expiresAt: string, regenerateIfActive: () => void) {
  clearAutoRefreshTimer();
  const ms = Math.max(0, new Date(expiresAt).getTime() - Date.now() - 500);
  autoRefreshTimer = setTimeout(() => {
    autoRefreshTimer = null;
    regenerateIfActive();
  }, ms);
}

export const usePairingStore = create<PairingState>()(
  (set, get) => ({
    current: { phase: "idle" },
    activeCode: null,
    codeError: null,
    incomingRequest: null,
    inboundQueue: [],

    async ensureActiveCode() {
      const { activeCode } = get();
      if (activeCode !== null) {
        const expired = new Date(activeCode.expiresAt).getTime() <= Date.now();
        if (!expired) return;
      }
      await get().generateCode();
    },

    async generateCode() {
      set({ codeError: null });
      try {
        const codeInfo = await commands.generatePairingCode(300); // 5 分钟
        set({
          activeCode: codeInfo,
          current: { phase: "generating", codeInfo },
          codeError: null,
        });
        scheduleAutoRefresh(codeInfo.expiresAt, () => {
          // 仅当还有活跃码时刷新（avoid 用户主动 clearActiveCode 后又被偷偷生成）
          if (get().activeCode !== null) {
            void get().generateCode();
          }
        });
      } catch (err) {
        if (handleNodeNotStarted(err)) return;
        const message = getErrorMessage(err);
        set({
          activeCode: null,
          codeError: message,
          current: { phase: "error", message },
        });
        clearAutoRefreshTimer();
        toast.error(message);
      }
    },

    async regenerateCode() {
      return get().generateCode();
    },

    clearActiveCode() {
      clearAutoRefreshTimer();
      set({ activeCode: null, codeError: null });
    },

    openInput() {
      set({ current: { phase: "inputting" } });
    },

    async searchDevice(code: string) {
      const version = ++searchVersion;
      set({ current: { phase: "searching", code } });
      try {
        const deviceInfo = await withTimeout(
          commands.getDeviceInfo(code),
          SEARCH_TIMEOUT_MS,
          t`查找设备`,
        );
        // 如果版本号不匹配，说明已被取消/重置
        if (searchVersion !== version) return;
        set({ current: { phase: "found", code, deviceInfo } });
      } catch (err) {
        if (searchVersion !== version) return;
        if (handleNodeNotStarted(err)) return;
        const message = getErrorMessage(err);
        set({ current: { phase: "error", message } });
        toast.error(message);
      }
    },

    async sendPairingRequest() {
      const { current } = get();
      if (current.phase !== "found") return;

      const { code, deviceInfo } = current;
      set({ current: { phase: "requesting", peerId: deviceInfo.peerId } });

      try {
        const response: PairingResponse = await withTimeout(
          commands.requestPairing(deviceInfo.peerId, { type: "code", code }, deviceInfo.codeRecord.listenAddrs ?? null),
          REQUEST_TIMEOUT_MS,
          t`配对请求`,
        );

        if (response.status === "success") {
          // 已配对设备由后端通过 paired-device-added 事件同步到运行时 store
          const displayName = deviceDisplayName(deviceInfo.codeRecord);
          set({
            current: {
              phase: "success",
              peerId: deviceInfo.peerId,
              deviceName: displayName,
            },
          });
          toast.success(t`已与 ${displayName} 配对成功`);
        } else {
          const message = getPairingRefuseMessage(response.reason);
          set({ current: { phase: "error", message } });
          toast.error(message);
        }
      } catch (err) {
        if (handleNodeNotStarted(err)) return;
        const message = getErrorMessage(err);
        set({ current: { phase: "error", message } });
        toast.error(message);
      }
    },

    handleInboundRequest(payload: PairingRequestPayload) {
      const { incomingRequest } = get();

      if (incomingRequest === null) {
        set({ incomingRequest: payload });
      } else {
        set((state) => ({
          inboundQueue: [...state.inboundQueue, payload],
        }));
      }
    },

    async acceptRequest() {
      const { incomingRequest } = get();
      if (!incomingRequest) return false;

      const { pendingId, osInfo, method } = incomingRequest;
      // 立即清空，防止双击导致重复发送响应（pending channel 只能消费一次）
      set({ incomingRequest: null });
      try {
        await commands.respondPairingRequest(
          pendingId,
          method,
          { status: "success" },
        );

        // 已配对设备由后端通过 paired-device-added 事件同步到运行时 store
        toast.success(t`已与 ${deviceDisplayName(osInfo)} 配对成功`);
        // 处理队列中的下一个请求
        get().processNextInbound();

        // Code 模式配对成功后，后端已消耗活跃码（单例设计）
        if (method.type === "code") {
          // 清理队列中其他 Code 模式请求——旧码已失效，继续展示只会报错
          set((state) => ({
            inboundQueue: state.inboundQueue.filter((r) => r.method.type !== "code"),
          }));
          // 活跃码被消耗：立即重生新的，下次开 UI 直接是新码
          if (get().activeCode !== null) {
            get().generateCode();
          }
        }
        return true;
      } catch (err) {
        if (handleNodeNotStarted(err)) return false;
        const message = getErrorMessage(err);
        toast.error(message);
        get().processNextInbound();
        return false;
      }
    },

    async rejectRequest() {
      const { incomingRequest } = get();
      if (!incomingRequest) return;

      const { pendingId, osInfo, method } = incomingRequest;
      // 立即清空，防止双击导致重复发送响应
      set({ incomingRequest: null });
      try {
        await commands.respondPairingRequest(
          pendingId,
          method,
          { status: "refused", reason: { type: "user_rejected" } },
        );
        toast.success(t`已拒绝来自 ${deviceDisplayName(osInfo)} 的配对请求`);
        // 处理队列中的下一个请求
        get().processNextInbound();
      } catch (err) {
        if (handleNodeNotStarted(err)) return;
        const message = getErrorMessage(err);
        toast.error(message);
        get().processNextInbound();
      }
    },

    async directPairing(peerId: PeerId) {
      set({ current: { phase: "requesting", peerId } });

      try {
        const response: PairingResponse = await withTimeout(
          commands.requestPairing(peerId, { type: "direct" }, null),
          REQUEST_TIMEOUT_MS,
          t`配对请求`,
        );

        if (response.status === "success") {
          // 已配对设备由后端通过 paired-device-added 事件同步到运行时 store
          const device = findNetworkDeviceSnapshot(peerId);
          const deviceName = device ? deviceDisplayName(device) : peerId.slice(-8);

          set({
            current: {
              phase: "success",
              peerId,
              deviceName,
            },
          });
          toast.success(t`已与 ${deviceName} 配对成功`);
        } else {
          const message = getPairingRefuseMessage(response.reason);
          set({ current: { phase: "error", message } });
          toast.error(message);
        }
      } catch (err) {
        if (handleNodeNotStarted(err)) return;
        const message = getErrorMessage(err);
        set({ current: { phase: "error", message } });
        toast.error(message);
      }
    },

    processNextInbound() {
      const { inboundQueue } = get();
      if (inboundQueue.length === 0) return;

      const [next, ...rest] = inboundQueue;
      set({
        incomingRequest: next,
        inboundQueue: rest,
      });
    },

    reset() {
      // 递增搜索版本以取消进行中的搜索
      searchVersion++;
      set({
        current: { phase: "idle" },
        incomingRequest: null,
        inboundQueue: [],
      });
      // 注意：不清 activeCode —— 配对码独立持久化，由 clearActiveCode 或
      // 节点停止/paired-device-added 事件管理。
    },

  }),
);
