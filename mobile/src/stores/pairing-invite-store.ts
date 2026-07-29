/**
 * Pairing Invite Store（移动端）
 *
 * PairInvite——一次性签名邀请，替代已废弃的 6 位配对码。
 * - 发起方：generateInvite → activeInvite（首页卡片展示二维码 + 复制），5min TTL 倒计时
 * - 受邀方：previewInvite（扫码/粘贴后解码验签看确认卡）→ confirmInvite（连接出示凭证）
 */

import type { MobileInvitePreview } from "react-native-swarmdrop-core";
import { create } from "zustand";
import { getMobileCore } from "@/core/mobile-core";

/** 邀请有效期（秒），与 core `INVITE_TTL_SECS` 一致 */
export const INVITE_TTL_SECS = 300;

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
  /** 解码验签邀请串 → 存 pending 供确认卡；返回是否成功（失败已 set error） */
  previewInvite: (invite: string) => Promise<boolean>;
  /** 确认后发起配对；返回 accepted */
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
        try {
          const preview = getMobileCore().decodePairInvite(v);
          if (Number(preview.expiresAt) * 1000 <= Date.now()) {
            set({ error: "邀请已过期", pending: null });
            return false;
          }
          set({ pending: { invite: v, preview }, error: null });
          return true;
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            pending: null,
          });
          return false;
        }
      },

      async confirmInvite() {
        const { pending } = get();
        if (!pending) return false;
        set({ confirming: true, error: null });
        try {
          const result = await getMobileCore().consumePairInvite(
            pending.invite,
          );
          if (result.accepted) {
            // 成功才清 pending——由确认页 router.replace(success) 导航，避免与
            // found-device 的「pending===null → back()」竞态。
            set({ confirming: false, pending: null });
          } else {
            // 拒绝：保留 pending，把 error 留在确认页展示（否则一闪即弹回主屏）。
            set({ confirming: false, error: result.reason ?? "配对被拒绝" });
          }
          return result.accepted;
        } catch (err) {
          set({
            confirming: false,
            error: err instanceof Error ? err.message : String(err),
          });
          return false;
        }
      },

      cancelPreview() {
        set({ pending: null, confirming: false, error: null });
      },
    };
  },
);
