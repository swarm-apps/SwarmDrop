/**
 * UpgradeLink Update Store（桌面端 only）
 *
 * 状态机字段 + action 委托给 [`lib/desktop-updater`](../lib/desktop-updater.ts) 服务。
 * Android 分支已随移动端迁出 SwarmDrop-RN 一并废弃。
 */

import { create } from "zustand";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { checkForUpdate, type UpgradeType } from "@/commands/upgrade";
import {
  fetchDesktopUpdate,
  runDesktopUpdate,
  type DownloadSnapshot,
} from "@/lib/desktop-updater";
import type { Update } from "@tauri-apps/plugin-updater";

export type UpgradeLinkStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "force-required"
  | "downloading"
  | "ready"
  | "error";

interface UpgradeLinkState {
  status: UpgradeLinkStatus;
  upgradeType: UpgradeType;
  latestVersion: string | null;
  currentVersion: string | null;
  promptContent: string | null;
  /** 更新日志 */
  releaseNotes: string | null;
  downloadUrl: string | null;
  progress: DownloadSnapshot | null;
  error: string | null;
  /** 是否已经检查过更新（避免重复检查） */
  hasChecked: boolean;

  // Actions
  checkForUpdate: (force?: boolean) => Promise<void>;
  executeUpdate: () => Promise<void>;
  openDownloadPage: () => Promise<void>;
  reset: () => void;
}

// 桌面端 Update 对象缓存：Tauri updater 的 Update 不能 JSON 序列化，所以放模块级。
let _pendingDesktopUpdate: Update | null = null;

export const useUpgradeLinkStore = create<UpgradeLinkState>()((set, get) => ({
  status: "idle",
  upgradeType: null,
  latestVersion: null,
  currentVersion: null,
  promptContent: null,
  releaseNotes: null,
  downloadUrl: null,
  progress: null,
  error: null,
  hasChecked: false,

  async checkForUpdate(force = false) {
    const { status, hasChecked } = get();
    if (!force && hasChecked) return;
    if (status === "checking" || status === "downloading") return;

    set({ status: "checking", error: null });

    try {
      const currentVersion = await getVersion();
      set({ currentVersion });

      const desktopUpdate = await fetchDesktopUpdate();
      if (!desktopUpdate) {
        set({ status: "up-to-date" });
        return;
      }

      _pendingDesktopUpdate = desktopUpdate;

      // UpgradeLink 策略（force / prompt / silent）
      const result = await checkForUpdate();

      set({
        latestVersion: desktopUpdate.version,
        upgradeType: result.upgradeType,
        releaseNotes: desktopUpdate.body ?? null,
        status: result.upgradeType === "force" ? "force-required" : "available",
      });
    } catch (err) {
      console.error("[upgrade] check failed:", err);
      set({
        status: "error",
        error: err instanceof Error ? err.message : String(err),
      });
    } finally {
      set({ hasChecked: true });
    }
  },

  async executeUpdate() {
    const { status } = get();
    if (status !== "available" && status !== "force-required") return;
    if (!_pendingDesktopUpdate) {
      set({ status: "error", error: "No update available" });
      return;
    }

    set({
      status: "downloading",
      progress: { downloaded: 0, total: 0, speed: 0, percent: 0 },
    });

    try {
      await runDesktopUpdate(_pendingDesktopUpdate, (snapshot) => {
        set({ progress: snapshot });
      });
      // Tauri updater 会自动重启
      set({ status: "ready" });
    } catch (err) {
      console.error("[upgrade] download failed:", err);
      set({
        status: "error",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  async openDownloadPage() {
    const { downloadUrl } = get();
    if (!downloadUrl) return;
    await openUrl(downloadUrl);
  },

  reset() {
    _pendingDesktopUpdate = null;
    set({
      status: "idle",
      upgradeType: null,
      latestVersion: null,
      currentVersion: null,
      promptContent: null,
      releaseNotes: null,
      downloadUrl: null,
      progress: null,
      error: null,
      hasChecked: false,
    });
  },
}));
