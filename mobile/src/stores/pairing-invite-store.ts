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
  ensureInvite: (localOnly?: boolean) => Promise<void>;
  regenerateInvite: (localOnly?: boolean) => Promise<void>;
  clearInvite: () => void;

  // 受邀方
  pending: InvitePreview | null;
  confirming: boolean;
  /** 解码验签邀请串 → 存 pending 供确认卡；返回是否成功（失败已 set error） */
  previewInvite: (invite: string) => Promise<boolean>;
  /** 确认后发起配对；返回 accepted */
  confirmInvite: () => Promise<boolean>;
  cancelPreview: () => void;
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
            void get().regenerateInvite(localOnly);
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

      async ensureInvite(localOnly = false) {
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

      async regenerateInvite(localOnly = false) {
        if (get().generating) return;
        await generate(localOnly);
      },

      clearInvite() {
        clearTimer();
        set({ activeInvite: null, error: null });
      },

      async previewInvite(invite: string) {
        const v = invite.trim();
        try {
          const preview = await getMobileCore().decodePairInvite(v);
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
          set({ confirming: false, pending: null });
          if (!result.accepted) {
            set({ error: result.reason ?? "配对被拒绝" });
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
