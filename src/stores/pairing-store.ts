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
} from "@/lib/bindings";
import type { PeerId } from "@/lib/types";
import { isErrorKind, getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import {
  findNetworkDeviceSnapshot,
  startNetworkFromStore,
} from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";

export type { PairingRequestPayload };

/** 配对请求超时（毫秒）——含连接握手 + 对端用户决策，给足时间 */
const REQUEST_TIMEOUT_MS = 30_000;

/** 邀请默认有效期（秒），与 core `INVITE_TTL_SECS` 一致（用于前端倒计时）。
 * 24h —— 链接分享是异步场景，5 分钟会让「发给同事、他十分钟后点开」必然失败
 * （openspec: invite-persistence）。改这里必须同步 crates/invite 的常量。 */
export const INVITE_TTL_SECS = 86_400;

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

/**
 * 配对成功后的提示。
 *
 * `persisted === false` 是一种**「一半成功」**：对端已经把本机加进它的已配对列表、本机
 * 这次运行内也能正常用，只是这条记录没写进钥匙串，重启后会不见。
 *
 * 它既不能报成失败（那会让两台设备对同一件事的认知永久分叉——对方明明成功了），
 * 也不能当成普通成功（用户不知道自己还得再配一次）。所以照实说。
 */
function notifyPaired(deviceName: string, persisted: boolean) {
  if (persisted) {
    toast.success(t`已与 ${deviceName} 配对成功`);
    return;
  }
  toast.warning(t`已与 ${deviceName} 配对成功`, {
    description: t`但这条记录没能保存下来，重启后需要重新配对。`,
  });
}

/**
 * 撤销本机发出的邀请——邀请是一次性信任凭证，UI 上「作废」了就该真的作废，
 * 不能只是从界面上消失、后端 registry 里还留着能用到 TTL 到点。
 *
 * fire-and-forget：后端幂等（不认识的串 no-op），节点未启动时 registry 本就是空的，
 * 任何失败都不影响调用方要的终态，所以不 await、不报错、不阻塞界面。
 *
 * **这里刻意忽略「是否已落盘」那个返回值**，与设置页的撤销按钮不同：这条路径是「生成新
 * 邀请时顺手作废旧的」，用户的注意力在新邀请上，为一条他已经不打算再用的旧邀请弹提示
 * 只是噪音。真要管理在外流通的邀请，去设置页的「已发出的邀请」——那里会报告落盘失败。
 */
function revokeInvite(invite: string | null): void {
  if (invite === null) return;
  void commands.revokePairInvite(invite).catch(() => {});
}

/** 使被 clear/后续生成覆盖的异步结果失效，避免旧请求晚到后复活已撤销邀请。 */
let inviteGenerationId = 0;

/** 本机生成的活跃邀请（发起方展示二维码/链接用） */
export interface ActiveInvite {
  /** 邀请串（canonical 链接形态，`https://swarm-apps.github.io/SwarmDrop/p/#…`；二维码就是它原样） */
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
  /** 正在生成邀请；用于禁止重复生成及复制已被撤销的旧链接 */
  isGeneratingInvite: boolean;
  /** 当前展示的入站配对请求 */
  incomingRequest: PairingRequestPayload | null;
  /** 入站请求队列 */
  inboundQueue: PairingRequestPayload[];

  // === 发起方：生成邀请 ===

  /** 确保活跃邀请存在（已有且未过期则 no-op；否则生成） */
  ensureActiveInvite: (localOnly?: boolean) => Promise<void>;
  /** 生成新邀请（强制覆盖现有；被覆盖的旧邀请立即撤销） */
  generateInvite: (localOnly?: boolean) => Promise<void>;
  /** 清空活跃邀请并撤销它（节点停止 / 用户主动 dismiss） */
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
  isGeneratingInvite: false,
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
    if (get().isGeneratingInvite) return;

    const generationId = ++inviteGenerationId;
    const previousInvite = get().activeInvite?.invite ?? null;
    set({
      activeInvite: null,
      inviteError: null,
      isGeneratingInvite: true,
    });
    // 先撤销再生成：UI 承诺「切换后此前展示的二维码随即失效」，这个失效不该
    // 取决于新邀请是否生成成功。生成期间也必须清掉旧串，否则页面仍能复制一条
    // 已被 registry 撤销的链接。
    revokeInvite(previousInvite);
    try {
      const invite = await commands.generatePairInvite(localOnly);
      if (generationId !== inviteGenerationId) {
        revokeInvite(invite);
        return;
      }
      set({
        activeInvite: { invite, generatedAt: Date.now(), localOnly },
        inviteError: null,
        isGeneratingInvite: false,
      });
    } catch (err) {
      if (generationId !== inviteGenerationId) return;
      set({ activeInvite: null, isGeneratingInvite: false });
      if (handleNodeNotStarted(err)) return;
      const message = getErrorMessage(err);
      set({ inviteError: message });
      toast.error(message);
    }
  },

  clearActiveInvite() {
    inviteGenerationId += 1;
    revokeInvite(get().activeInvite?.invite ?? null);
    set({ activeInvite: null, inviteError: null, isGeneratingInvite: false });
  },

  /**
   * 解码一条邀请并进入确认态。
   *
   * ## 三道本地过滤，全部在出网之前
   *
   * 解码是纯本地计算（不拨号、不查 DHT），所以「确认卡出现之前零出网」这条成立。
   * 过期、**自我邀请**、**已配对**三条都在这里拦下：
   *
   * 后两条此前只存在于剪贴板感知那条路径（`use-clipboard-invite.ts`），而**手动粘贴
   * 完全绕过**——用户把自己刚复制、准备发给别人的邀请误粘进 `/pairing/input`，桌面会
   * 一路放行到「确认配对」，点下去才由内核报错。Web 端把三条来路（手打 / 剪贴板 /
   * 落地页 handoff）全部收口在同一个函数里，这里跟上。
   *
   * 判据用签名覆盖范围内的 `peerId`，伪造不了；比对「当前生成的那一串」则漏得掉历史
   * 生成的、以及重启之后的（那时本地根本没有那串可比）。
   */
  async previewInvite(invite: string) {
    try {
      const preview = await commands.decodePairInvite(invite.trim());
      if (preview.expiresAt * 1000 <= Date.now()) {
        const message = t`邀请已过期`;
        set({ current: { phase: "idle" } });
        toast.error(message);
        return;
      }

      // 判据取 `secret-store.deviceId`（= 本机 `node_id`，`initializeIdentity()` 在首启
      // 就写好了），**不取 `networkStatus.peerId`**：后者只有节点跑起来之后才有值，而
      // 「粘一条邀请看看是谁的」在节点没起来时照样可达——那时过滤会静默失效，用户对着
      // 自己发的邀请走完确认卡，再撞上一句无从解释的配对失败。
      const selfPeerId = useSecretStore.getState().deviceId;
      if (selfPeerId !== null && preview.peerId === selfPeerId) {
        set({ current: { phase: "idle" } });
        toast.info(t`这是本机生成的邀请`, {
          description: t`把它发给对方，不是自己用。`,
        });
        return;
      }

      const paired = useSecretStore.getState().pairedDevices;
      const already = paired.find((device) => device.peerId === preview.peerId);
      if (already) {
        set({ current: { phase: "idle" } });
        toast.info(t`已经和这台设备配过对了`, {
          description: t`去「发送」页直接选它就行。`,
        });
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
      const { response, persisted } = await withTimeout(
        commands.consumePairInvite(invite),
        REQUEST_TIMEOUT_MS,
        t`配对请求`,
      );

      if (response.status === "success") {
        const deviceName = preview.displayName || preview.peerId.slice(-8);
        set({ current: { phase: "success" } });
        notifyPaired(deviceName, persisted);
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
      const persisted = await commands.respondPairingRequest(pendingId, method, {
        status: "success",
      });
      notifyPaired(deviceDisplayName(osInfo), persisted);
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
      const { response, persisted } = await withTimeout(
        commands.requestPairing(peerId, { type: "direct" }, null),
        REQUEST_TIMEOUT_MS,
        t`配对请求`,
      );

      if (response.status === "success") {
        const device = findNetworkDeviceSnapshot(peerId);
        const deviceName = device ? deviceDisplayName(device) : peerId.slice(-8);
        set({ current: { phase: "success" } });
        notifyPaired(deviceName, persisted);
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
