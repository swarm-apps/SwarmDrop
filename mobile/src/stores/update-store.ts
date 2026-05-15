/**
 * Android in-app update state machine.
 *
 *      ┌──────┐ checkForUpdate                    executeUpdate
 *      │ idle │──────────▶ checking ──┬─▶ up-to-date               ┌──▶ ready
 *      └──────┘                       │                            │
 *          ▲                          ├─▶ available ─────────────▶ │
 *          │ reset                    └─▶ force-required ────────▶ │ downloading
 *          │                                                       │
 *          │                                                       └─▶ error
 *          └────────── from any state via reset()
 *
 * iOS：Platform.OS !== 'android' → checkForUpdate() 直接 no-op，
 * status 一直留在 'idle'，UI 也就不会挂载。
 */

import AsyncStorage from "@react-native-async-storage/async-storage";
import Constants from "expo-constants";
import { AppState, type AppStateStatus, Platform } from "react-native";
import { create } from "zustand";
import {
  type DownloadProgress,
  downloadAndInstallApk,
} from "@/core/update-installer";
import {
  type AndroidUpdateResult,
  checkAndroidUpdate,
  semverToVersionCode,
  type UpgradeType,
} from "@/core/upgrade-client";

const DISMISS_KEY = "update-dismiss";
const RECHECK_INTERVAL_MS = 12 * 60 * 60 * 1000; // 12 hours
const DISMISS_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours

export type UpdateStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "force-required"
  | "downloading"
  | "ready"
  | "error";

interface DismissRecord {
  tag: string;
  dismissedAt: number;
}

interface UpdateState {
  status: UpdateStatus;
  upgradeType: UpgradeType;
  latestVersion: string | null;
  currentVersion: string | null;
  promptContent: string | null;
  releaseNotes: string | null;
  downloadUrl: string | null;
  progress: DownloadProgress | null;
  error: string | null;
  hasChecked: boolean;
  lastCheckedAt: number;
  /** 用户在前台进度对话框选择"后台下载"后置 true，UI 切换为 toast 形态。
   *  每次 executeUpdate() 入口处会重置为 false。 */
  backgrounded: boolean;

  checkForUpdate(force?: boolean): Promise<void>;
  executeUpdate(): Promise<void>;
  /** 关掉前台进度对话框但不中止下载 */
  backgroundDownload(): void;
  /** 用户读完错误提示后调用，把状态翻回 "available" 让 UpdateDialog 再次弹出 */
  acknowledgeError(): void;
  dismiss(): Promise<void>;
  reset(): void;
  setupAppStateListener(): () => void;
}

async function readDismiss(): Promise<DismissRecord | null> {
  try {
    const raw = await AsyncStorage.getItem(DISMISS_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as DismissRecord;
    if (
      typeof parsed.tag !== "string" ||
      typeof parsed.dismissedAt !== "number"
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

async function writeDismiss(tag: string): Promise<void> {
  try {
    const record: DismissRecord = { tag, dismissedAt: Date.now() };
    await AsyncStorage.setItem(DISMISS_KEY, JSON.stringify(record));
  } catch (err) {
    console.warn("[update] failed to persist dismiss", err);
  }
}

export const useUpdateStore = create<UpdateState>()((set, get) => ({
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
  lastCheckedAt: 0,
  backgrounded: false,

  async checkForUpdate(force = false) {
    if (Platform.OS !== "android") return;

    const { status, lastCheckedAt } = get();
    if (status === "checking" || status === "downloading") return;
    if (
      !force &&
      lastCheckedAt > 0 &&
      Date.now() - lastCheckedAt < RECHECK_INTERVAL_MS
    ) {
      return;
    }

    set({ status: "checking", error: null });

    const currentVersion = Constants.expoConfig?.version ?? "0.0.0";
    const currentVersionCode = semverToVersionCode(currentVersion);

    let result: AndroidUpdateResult;
    try {
      result = await checkAndroidUpdate(currentVersionCode);
    } catch (err) {
      console.warn("[update] check failed", err);
      set({
        status: "up-to-date",
        currentVersion,
        hasChecked: true,
        lastCheckedAt: Date.now(),
        error: err instanceof Error ? err.message : String(err),
      });
      return;
    }

    const updates: Partial<UpdateState> = {
      currentVersion,
      hasChecked: true,
      lastCheckedAt: Date.now(),
    };

    if (!result.hasUpdate) {
      set({ ...updates, status: "up-to-date" });
      return;
    }

    // 强制升级路径绕开 dismiss 缓存
    if (result.upgradeType !== "force") {
      const dismiss = await readDismiss();
      if (
        dismiss &&
        dismiss.tag === result.versionName &&
        Date.now() - dismiss.dismissedAt < DISMISS_TTL_MS
      ) {
        set({ ...updates, status: "up-to-date" });
        return;
      }
    }

    set({
      ...updates,
      status: result.upgradeType === "force" ? "force-required" : "available",
      upgradeType: result.upgradeType,
      latestVersion: result.versionName,
      downloadUrl: result.downloadUrl,
      promptContent: result.promptContent,
      releaseNotes: result.promptContent,
    });
  },

  async executeUpdate() {
    const { status, downloadUrl } = get();
    if (status !== "available" && status !== "force-required") return;
    if (!downloadUrl) {
      set({ status: "error", error: "No download URL available" });
      return;
    }

    set({
      status: "downloading",
      backgrounded: false,
      progress: { downloaded: 0, total: 0, percent: 0 },
    });

    try {
      await downloadAndInstallApk(downloadUrl, (progress) => {
        set({ progress });
      });
      // install intent 一旦投递，控制权交给系统 UI；用户点确认应用进程会被
      // 替换；若取消，下次 checkForUpdate 会再次弹出 prompt。
      set({ status: "ready" });
    } catch (err) {
      console.error("[update] install failed", err);
      set({
        status: "error",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  backgroundDownload() {
    if (get().status !== "downloading") return;
    set({ backgrounded: true });
  },

  acknowledgeError() {
    if (get().status !== "error") return;
    // 回到 "available" 让原来的 UpdateDialog 再次弹出，用户可以再次点"立即更新"。
    // 即使原状态是 "force-required"，回 "available" 至少能让用户看到 prompt；
    // 反正 downloadUrl 不变，executeUpdate 会再走一次完整流程。
    set({
      status: "available",
      error: null,
      progress: null,
      backgrounded: false,
    });
  },

  async dismiss() {
    const { latestVersion, upgradeType } = get();
    if (upgradeType === "force") return; // 强制升级不允许 dismiss
    // 同步关闭对话框，AsyncStorage 异步落盘失败也只影响下次启动行为，
    // 已经在 writeDismiss 内吞掉了。
    set({ status: "up-to-date" });
    if (latestVersion) {
      void writeDismiss(latestVersion);
    }
  },

  reset() {
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
      lastCheckedAt: 0,
      backgrounded: false,
    });
  },

  setupAppStateListener() {
    const handler = (state: AppStateStatus) => {
      if (state !== "active") return;
      void get().checkForUpdate();
    };
    const sub = AppState.addEventListener("change", handler);
    return () => sub.remove();
  },
}));
