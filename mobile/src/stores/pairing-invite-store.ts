/**
 * Pairing Invite Store（移动端）
 *
 * PairInvite——一次性签名邀请，替代已废弃的 6 位配对码。
 * - 发起方：generateInvite → activeInvite（首页卡片展示二维码 + 复制），24h TTL 倒计时
 * - 受邀方：previewInvite（扫码/粘贴后解码验签看确认卡）→ confirmInvite（连接出示凭证）
 */

import type { MobileInvitePreview } from "react-native-swarmdrop-core";
import { create } from "zustand";
import { getMobileCore } from "@/core/mobile-core";

/**
 * 邀请有效期（秒），与 core `INVITE_TTL_SECS` 一致 —— **改这里必须同步 Rust 常量**。
 *
 * 曾经是 300（5 分钟），那对「当面扫码」够用，但载体变成可分享链接后是灾难：
 * `scheduleRefresh` 会在 TTL 到点时重新生成，而生成前**先撤销旧邀请** ——
 * 于是用户发出去的链接会在 5 分钟后被自己的 App 悄悄作废，对方点开被拒。
 * 那正是 invite-persistence 把 TTL 放宽到 24h 要解决的场景，这一端漏改就把它抵消了。
 */
export const INVITE_TTL_SECS = 86_400;

/**
 * `previewInvite` 被拒的分类 —— 语言中立判别码，文案由 UI 层本地化。
 *
 * - `expired`：解码成功但已过期
 * - `self`：这是本机自己发出的邀请（用户复制了准备发给别人的那条）
 * - `invalid`：解析 / 验签失败，或已被消费 / 已撤销
 */
export type PreviewReject = "expired" | "self" | "invalid";

/**
 * `confirmInvite` 被拒的分类。
 *
 * - `userRejected`：对端用户点了拒绝（后端判别码 `user_rejected`）
 * - `failed`：连接 / 协议层失败，或后端给了本端不认识的判别码
 */
export type ConfirmReject = "userRejected" | "failed";

/** 本机活跃邀请（发起方展示二维码/链接） */
export interface ActiveInvite {
  invite: string;
  /** 生成时刻（毫秒），倒计时基准 */
  generatedAt: number;
  localOnly: boolean;
}

/** 受邀方预览状态（解码验签后的确认卡数据） */
export interface InvitePreview {
  invite: string;
  preview: MobileInvitePreview;
}

interface PairingInviteState {
  // 发起方
  activeInvite: ActiveInvite | null;
  generating: boolean;
  error: string | null;
  ensureActiveInvite: (localOnly?: boolean) => Promise<void>;
  /** 生成新邀请（被覆盖的旧邀请立即撤销） */
  generateInvite: (localOnly?: boolean) => Promise<void>;
  /** 清空活跃邀请并撤销它 */
  clearActiveInvite: () => void;

  // 受邀方
  pending: InvitePreview | null;
  confirming: boolean;
  /**
   * 本机 peerId，由 `mobile-core-store` 在身份就绪后推入（见那里的注释）。
   * 只用于自我过滤；`null` 表示身份还没加载好，此时不过滤（宁可放过也别误拦）。
   */
  selfPeerId: string | null;
  /**
   * 上一次 `previewInvite` 被拒的**分类**（成功或未调用则为 `null`）。
   *
   * 不把文案塞进 `error`：那条路径原先是「硬编码中文 + core 透传的 Rust 错误串」，
   * 英文界面下会直接露出中文。判别码留在 store、文案交给有 `t` 的 UI 层 —— 与桌面端
   * `kind → KIND_MESSAGES` 的做法一致。技术细节只进 console，不进 UI。
   */
  previewReject: PreviewReject | null;
  /** 解码验签邀请串 → 存 pending 供确认卡；返回是否成功（失败原因见 `previewReject`） */
  previewInvite: (invite: string) => Promise<boolean>;
  /**
   * 上一次 `confirmInvite` 被拒的**分类**（成功或未调用则为 `null`）。
   *
   * 与 `previewReject` 同理：判别码留在 store、文案交给 UI。后端给的 `reason` 是稳定的
   * snake_case 码（不再是 `{reason:?}` 的 Rust 裸标识符），未知码一律降级到通用文案。
   */
  confirmReject: ConfirmReject | null;
  /** 确认后发起配对；返回 accepted（失败原因见 `confirmReject`） */
  confirmInvite: () => Promise<boolean>;
  cancelPreview: () => void;
}

/**
 * 撤销本机发出的邀请——邀请是一次性信任凭证，UI 上「作废」了就该真的作废，
 * 不能只是从界面上消失、后端 registry 里还留着能用到 TTL 到点。
 *
 * fire-and-forget：后端幂等（不认识的串 no-op），节点未启动时 registry 本就是空的，
 * 任何失败都不影响调用方要的终态，所以不 await、不报错、不阻塞界面。
 */
function revokeInvite(active: ActiveInvite | null): void {
  if (active === null) return;
  void getMobileCore()
    .revokePairInvite(active.invite)
    .catch(() => {});
}

let autoRefreshTimer: ReturnType<typeof setTimeout> | null = null;

function clearTimer() {
  if (autoRefreshTimer !== null) {
    clearTimeout(autoRefreshTimer);
    autoRefreshTimer = null;
  }
}

function scheduleRefresh(generatedAt: number, regenerateIfActive: () => void) {
  clearTimer();
  const ms = Math.max(
    0,
    generatedAt + INVITE_TTL_SECS * 1000 - Date.now() - 500,
  );
  autoRefreshTimer = setTimeout(() => {
    autoRefreshTimer = null;
    regenerateIfActive();
  }, ms);
}

function isExpired(inv: ActiveInvite): boolean {
  return inv.generatedAt + INVITE_TTL_SECS * 1000 <= Date.now();
}

export const usePairingInviteStore = create<PairingInviteState>()(
  (set, get) => {
    async function generate(localOnly: boolean): Promise<void> {
      set({ generating: true, error: null });
      // 先撤销再生成：失效是「重新生成」当场承诺的效果，不该取决于新邀请是否成功
      // （失败路径下 activeInvite 同样被清空，状态一致）。
      revokeInvite(get().activeInvite);
      try {
        const invite = await getMobileCore().generatePairInvite(localOnly);
        const active: ActiveInvite = {
          invite,
          generatedAt: Date.now(),
          localOnly,
        };
        set({ activeInvite: active, generating: false, error: null });
        scheduleRefresh(active.generatedAt, () => {
          const { activeInvite, generating } = get();
          if (activeInvite !== null && !generating)
            void get().generateInvite(localOnly);
        });
      } catch (err) {
        clearTimer();
        set({
          activeInvite: null,
          generating: false,
          error: err instanceof Error ? err.message : String(err),
        });
        console.warn("[pairing-invite] generate failed:", err);
      }
    }

    return {
      activeInvite: null,
      generating: false,
      error: null,
      pending: null,
      confirming: false,
      selfPeerId: null,
      previewReject: null,
      confirmReject: null,

      async ensureActiveInvite(localOnly = false) {
        const { activeInvite, generating } = get();
        if (generating) return;
        if (
          activeInvite !== null &&
          activeInvite.localOnly === localOnly &&
          !isExpired(activeInvite)
        )
          return;
        await generate(localOnly);
      },

      async generateInvite(localOnly = false) {
        if (get().generating) return;
        await generate(localOnly);
      },

      clearActiveInvite() {
        clearTimer();
        revokeInvite(get().activeInvite);
        set({ activeInvite: null, error: null });
      },

      async previewInvite(invite: string) {
        const v = invite.trim();
        set({ previewReject: null });
        try {
          const preview = getMobileCore().decodePairInvite(v);
          if (Number(preview.expiresAt) * 1000 <= Date.now()) {
            set({ pending: null, previewReject: "expired" });
            return false;
          }
          // 自我过滤：用户复制自己刚生成的邀请准备发给别人，回头又粘回来（或在本机点开
          // 自己分享的链接）。判据取签名覆盖范围内的结构性字段 `peerId`，不是展示名。
          const { selfPeerId } = get();
          if (selfPeerId !== null && preview.peerId === selfPeerId) {
            set({ pending: null, previewReject: "self" });
            return false;
          }
          set({ pending: { invite: v, preview }, error: null });
          return true;
        } catch (err) {
          // 技术细节（`FfiError.InvalidCode`、解析失败在哪一步）只进 console：它对用户
          // 没有意义，而且是 Rust 侧的中文串。UI 只需要知道「这条邀请不能用」。
          console.warn("[pairing] previewInvite 失败:", err);
          set({ pending: null, previewReject: "invalid" });
          return false;
        }
      },

      async confirmInvite() {
        const { pending } = get();
        if (!pending) return false;
        set({ confirming: true, error: null, confirmReject: null });
        try {
          const result = await getMobileCore().consumePairInvite(
            pending.invite,
          );
          if (result.accepted) {
            // 成功才清 pending——由确认页 router.replace(success) 导航，避免与
            // found-device 的「pending===null → back()」竞态。
            set({ confirming: false, pending: null });
          } else {
            // 拒绝：保留 pending，让确认页展示原因（否则一闪即弹回主屏）。
            set({
              confirming: false,
              confirmReject:
                result.reason === "user_rejected" ? "userRejected" : "failed",
            });
          }
          return result.accepted;
        } catch (err) {
          // 技术细节只进 console —— 抛上来的是 Rust 侧的错误串。
          console.warn("[pairing] confirmInvite 失败:", err);
          set({ confirming: false, confirmReject: "failed" });
          return false;
        }
      },

      cancelPreview() {
        set({
          pending: null,
          confirming: false,
          error: null,
          previewReject: null,
          confirmReject: null,
        });
      },
    };
  },
);
