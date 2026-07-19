/**
 * Pairing Store
 * 管理配对流程状态（PairInvite——一次性签名邀请，替代已废弃的 6 位配对码）。
 *
 * 出站两条路径：
 * - 邀请（跨网/扫码）：generateInvite → 展示二维码/链接；受邀方 previewInvite（解码验签
 *   看确认卡）→ consumeInvite（连接 + 出示凭证）。
 * - Direct（同局域网点按）：directPairing（对端 LAN mDNS 校验）。
 * 入站请求（作为邀请发起方收到受邀方连接）走 incomingRequest + accept/reject。
 */

import { create } from "zustand";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import {
  commands,
  type PairInvitePreview,
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

/** 配对请求超时（毫秒）——含连接握手 + 对端用户决策，给足时间 */
const REQUEST_TIMEOUT_MS = 30_000;

/** 邀请默认有效期（秒），与 core `INVITE_TTL_SECS` 一致（用于前端倒计时） */
export const INVITE_TTL_SECS = 300;

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

/** 本机生成的活跃邀请（发起方展示二维码/链接用） */
export interface ActiveInvite {
  /** 邀请串（小写规范形态，"sdinvite..."） */
  invite: string;
  /** 生成时刻（毫秒），倒计时基准；有效期 = generatedAt + INVITE_TTL_SECS */
  generatedAt: number;
  /** LocalOnly 策略 */
  localOnly: boolean;
}

/** 出站配对阶段（受邀方侧：预览确认 → 请求中 → 成功）。
 * 错误只经 toast 传达、不进状态机（UI 无 error 态渲染）；成功/请求态无 payload（UI 只读 phase）。 */
export type PairingPhase =
  | { phase: "idle" }
  | { phase: "previewing"; invite: string; preview: PairInvitePreview }
  | { phase: "requesting" }
  | { phase: "success" };

interface PairingState {
  /** 当前出站配对阶段 */
  current: PairingPhase;
  /** 本机活跃邀请（发起方），跨页面/弹窗持久化 */
  activeInvite: ActiveInvite | null;
  /** 生成邀请时的错误（瞬时；下一次 generate 清空） */
  inviteError: string | null;
  /** 当前展示的入站配对请求 */
  incomingRequest: PairingRequestPayload | null;
  /** 入站请求队列 */
  inboundQueue: PairingRequestPayload[];

  // === 发起方：生成邀请 ===

  /** 确保活跃邀请存在（已有且未过期则 no-op；否则生成） */
  ensureActiveInvite: (localOnly?: boolean) => Promise<void>;
  /** 生成新邀请（强制覆盖现有） */
  generateInvite: (localOnly?: boolean) => Promise<void>;
  /** 清空活跃邀请（节点停止 / 用户主动 dismiss） */
  clearActiveInvite: () => void;

  // === 受邀方：预览 + 消费 ===

  /** 解码验签邀请串 → 展示确认卡（不发起配对；篡改/过期在此拒） */
  previewInvite: (invite: string) => Promise<void>;
  /** 确认后发起配对（连接 + 出示凭证一步到位） */
  confirmInvite: () => Promise<void>;

  // === 入站请求 + Direct ===

  handleInboundRequest: (payload: PairingRequestPayload) => void;
  acceptRequest: () => Promise<boolean>;
  rejectRequest: () => Promise<void>;
  /** Direct 模式配对（同局域网点按直连） */
  directPairing: (peerId: PeerId) => Promise<void>;
  processNextInbound: () => void;
  reset: () => void;
}

export const usePairingStore = create<PairingState>()((set, get) => ({
  current: { phase: "idle" },
  activeInvite: null,
  inviteError: null,
  incomingRequest: null,
  inboundQueue: [],

  async ensureActiveInvite(localOnly = false) {
    const { activeInvite } = get();
    if (activeInvite !== null && activeInvite.localOnly === localOnly) {
      const expiresAt = activeInvite.generatedAt + INVITE_TTL_SECS * 1000;
      if (expiresAt > Date.now()) return;
    }
    await get().generateInvite(localOnly);
  },

  async generateInvite(localOnly = false) {
    set({ inviteError: null });
    try {
      const invite = await commands.generatePairInvite(localOnly);
      set({
        activeInvite: { invite, generatedAt: Date.now(), localOnly },
        inviteError: null,
      });
    } catch (err) {
      if (handleNodeNotStarted(err)) return;
      const message = getErrorMessage(err);
      set({ activeInvite: null, inviteError: message });
      toast.error(message);
    }
  },

  clearActiveInvite() {
    set({ activeInvite: null, inviteError: null });
  },

  async previewInvite(invite: string) {
    try {
      const preview = await commands.decodePairInvite(invite.trim());
      if (preview.expiresAt * 1000 <= Date.now()) {
        const message = t`邀请已过期`;
        set({ current: { phase: "idle" } });
        toast.error(message);
        return;
      }
      set({ current: { phase: "previewing", invite: invite.trim(), preview } });
    } catch (err) {
      const message = getErrorMessage(err);
      set({ current: { phase: "idle" } });
      toast.error(message);
    }
  },

  async confirmInvite() {
    const { current } = get();
    if (current.phase !== "previewing") return;
    const { invite, preview } = current;
    set({ current: { phase: "requesting" } });

    try {
      const response: PairingResponse = await withTimeout(
        commands.consumePairInvite(invite),
        REQUEST_TIMEOUT_MS,
        t`配对请求`,
      );

      if (response.status === "success") {
        const deviceName = preview.displayName || preview.peerId.slice(-8);
        set({ current: { phase: "success" } });
        toast.success(t`已与 ${deviceName} 配对成功`);
      } else {
        const message = getPairingRefuseMessage(response.reason);
        set({ current: { phase: "idle" } });
        toast.error(message);
      }
    } catch (err) {
      if (handleNodeNotStarted(err)) return;
      const message = getErrorMessage(err);
      set({ current: { phase: "idle" } });
      toast.error(message);
    }
  },

  handleInboundRequest(payload: PairingRequestPayload) {
    const { incomingRequest } = get();
    if (incomingRequest === null) {
      set({ incomingRequest: payload });
    } else {
      set((state) => ({ inboundQueue: [...state.inboundQueue, payload] }));
    }
  },

  async acceptRequest() {
    const { incomingRequest } = get();
    if (!incomingRequest) return false;

    const { pendingId, osInfo, method } = incomingRequest;
    // 立即清空，防止双击重复响应（pending channel 只能消费一次）
    set({ incomingRequest: null });
    try {
      await commands.respondPairingRequest(pendingId, method, { status: "success" });
      const deviceName = deviceDisplayName(osInfo);
      toast.success(t`已与 ${deviceName} 配对成功`);
      get().processNextInbound();
      return true;
    } catch (err) {
      if (handleNodeNotStarted(err)) return false;
      toast.error(getErrorMessage(err));
      get().processNextInbound();
      return false;
    }
  },

  async rejectRequest() {
    const { incomingRequest } = get();
    if (!incomingRequest) return;

    const { pendingId, osInfo, method } = incomingRequest;
    set({ incomingRequest: null });
    try {
      await commands.respondPairingRequest(pendingId, method, {
        status: "refused",
        reason: { type: "user_rejected" },
      });
      toast.success(t`已拒绝来自 ${deviceDisplayName(osInfo)} 的配对请求`);
      get().processNextInbound();
    } catch (err) {
      if (handleNodeNotStarted(err)) return;
      toast.error(getErrorMessage(err));
      get().processNextInbound();
    }
  },

  async directPairing(peerId: PeerId) {
    set({ current: { phase: "requesting" } });
    try {
      const response: PairingResponse = await withTimeout(
        commands.requestPairing(peerId, { type: "direct" }, null),
        REQUEST_TIMEOUT_MS,
        t`配对请求`,
      );

      if (response.status === "success") {
        const device = findNetworkDeviceSnapshot(peerId);
        const deviceName = device ? deviceDisplayName(device) : peerId.slice(-8);
        set({ current: { phase: "success" } });
        toast.success(t`已与 ${deviceName} 配对成功`);
      } else {
        const message = getPairingRefuseMessage(response.reason);
        set({ current: { phase: "idle" } });
        toast.error(message);
      }
    } catch (err) {
      if (handleNodeNotStarted(err)) return;
      const message = getErrorMessage(err);
      set({ current: { phase: "idle" } });
      toast.error(message);
    }
  },

  processNextInbound() {
    const { inboundQueue } = get();
    if (inboundQueue.length === 0) return;
    const [next, ...rest] = inboundQueue;
    set({ incomingRequest: next, inboundQueue: rest });
  },

  reset() {
    set({ current: { phase: "idle" }, incomingRequest: null, inboundQueue: [] });
    // 不清 activeInvite——邀请独立持久化，由 clearActiveInvite / 节点停止管理。
  },
}));
