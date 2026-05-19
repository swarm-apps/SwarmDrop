/**
 * Pairing Code Store
 *
 * 全局单例的配对码管理：
 *   - 跨 PairingSheet 关闭/重开持久化（不再是组件 local state）
 *   - 过期前自动续生（5min TTL，提前 500ms 拿新码避免 UI 闪 "已过期"）
 *   - 被消耗后（accept 入站 code 请求）自动续生（caller 调用 markConsumed）
 *
 * 后端 share code 单例设计：reject 不消耗码（参考 SwarmDrop core
 * pairing/manager.rs:271-285）；本 store 仅在 accept 时触发 markConsumed。
 */

import { create } from "zustand";
import { getMobileCore } from "@/core/mobile-core";
import type { MobilePairingCode } from "react-native-swarmdrop-core";

const TTL_SECS = 600n;

interface PairingCodeState {
  /** 当前活跃配对码；null 表示未生成或已 clear */
  codeInfo: MobilePairingCode | null;
  generating: boolean;
  error: string | null;

  /** 确保有有效活跃码（已有且未过期 → no-op；否则 generate） */
  ensure: () => Promise<void>;
  /** 强制重生（用户主动点"刷新"按钮） */
  regenerate: () => Promise<void>;
  /** 清空 + 取消自动刷新（节点停止 / 用户主动 dismiss） */
  clear: () => void;
  /** 标记码已被消耗（accept code 请求后），自动续生 */
  markConsumed: () => void;
}

let autoRefreshTimer: ReturnType<typeof setTimeout> | null = null;

function clearTimer() {
  if (autoRefreshTimer !== null) {
    clearTimeout(autoRefreshTimer);
    autoRefreshTimer = null;
  }
}

function scheduleRefresh(expiresAt: bigint) {
  clearTimer();
  // expiresAt 单位是 seconds (i64)，转毫秒；提前 500ms 重生
  const ms = Math.max(0, Number(expiresAt) * 1000 - Date.now() - 500);
  autoRefreshTimer = setTimeout(() => {
    autoRefreshTimer = null;
    if (usePairingCodeStore.getState().codeInfo !== null) {
      usePairingCodeStore.getState().regenerate();
    }
  }, ms);
}

function isExpired(info: MobilePairingCode): boolean {
  return Number(info.expiresAt) * 1000 <= Date.now();
}

async function doGenerate(): Promise<void> {
  usePairingCodeStore.setState({ generating: true, error: null });
  try {
    const info = await getMobileCore().generatePairingCode(TTL_SECS);
    usePairingCodeStore.setState({
      codeInfo: info,
      generating: false,
      error: null,
    });
    scheduleRefresh(info.expiresAt);
  } catch (err) {
    clearTimer();
    usePairingCodeStore.setState({
      codeInfo: null,
      generating: false,
      error: err instanceof Error ? err.message : String(err),
    });
    console.warn("[pairing-code] generate failed:", err);
  }
}

export const usePairingCodeStore = create<PairingCodeState>()(() => ({
  codeInfo: null,
  generating: false,
  error: null,

  async ensure() {
    const { codeInfo, generating } = usePairingCodeStore.getState();
    if (generating) return;
    if (codeInfo !== null && !isExpired(codeInfo)) return;
    await doGenerate();
  },

  async regenerate() {
    if (usePairingCodeStore.getState().generating) return;
    await doGenerate();
  },

  clear() {
    clearTimer();
    usePairingCodeStore.setState({ codeInfo: null, error: null });
  },

  markConsumed() {
    // 仅在有活跃码时续生；没有码就不主动生成（用户没在用配对码功能）
    if (usePairingCodeStore.getState().codeInfo !== null) {
      void doGenerate();
    }
  },
}));
