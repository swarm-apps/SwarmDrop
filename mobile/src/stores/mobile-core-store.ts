import AsyncStorage from "@react-native-async-storage/async-storage";
import type {
  MobileDevice as DeviceInfo,
  MobileNetworkStatus as NetworkStatus,
  MobileTransferFile as TransferFile,
} from "react-native-swarmdrop-core";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { pickTransferFiles } from "@/core/file-access";
import { initMobileCore } from "@/core/mobile-core";
import { usePreferencesStore } from "@/stores/preferences-store";

export type RuntimeState = "stopped" | "starting" | "running" | "error";
/** 设备身份加载状态 —— UI 端用 i18n 渲染对应文案 */
export type IdentityStatus = "idle" | "loading" | "ready" | "failed";

/** 已配对设备的骨架信息 —— 节点未启动时也能显示 */
export interface PairedDeviceSummary {
  peerId: string;
  hostname: string;
  os: string;
  platform: string;
  arch: string;
}

// Rust 端 networkStatus.status 是任意字符串,RN 用 union 类型,需要收敛
function toRuntimeState(status: string): RuntimeState {
  return status === "running" ? "running" : "stopped";
}

function toPairedSummaries(devices: DeviceInfo[]): PairedDeviceSummary[] {
  return devices
    .filter((d) => d.isPaired)
    .map((d) => ({
      peerId: d.peerId,
      hostname: d.hostname,
      os: d.os,
      platform: d.platform,
      arch: d.arch,
    }));
}

type MobileCoreState = {
  identityStatus: IdentityStatus;
  peerId: string | null;
  runtimeState: RuntimeState;
  networkStatus: NetworkStatus | null;
  /** 实时设备列表 —— Rust list_devices 依赖 NetManager,仅节点 running 时有数据 */
  devices: DeviceInfo[];
  /** 已配对设备的持久化骨架 —— 节点未启动时主屏 fallback 用 */
  pairedDevicesCache: PairedDeviceSummary[];
  selectedFiles: TransferFile[];
  error: string | null;
  initialized: boolean;
  /** 节点最近一次 running 的开始时间(ms),用于"运行时长"计算;非 running 时为 null */
  startedAt: number | null;
  /** 仅加载身份(获取 peerId),不启动 P2P 节点。第一次进入主屏调一次。 */
  loadIdentity: () => Promise<void>;
  /** 关闭 NetManager 释放 P2P 资源（进入后台时调用） */
  shutdownNode: () => Promise<void>;
  /** 启动 NetManager（前台恢复时调用） */
  startNode: () => Promise<void>;
  refreshDevices: () => Promise<void>;
  refreshNetworkStatus: () => Promise<void>;
  chooseFiles: () => Promise<void>;
  setSelectedFiles: (files: TransferFile[]) => void;
  clearSelectedFiles: () => void;
  applyNetworkStatus: (status: NetworkStatus) => void;
  applyDevices: (devices: DeviceInfo[]) => void;
  setError: (error: string | null) => void;
};

export const useMobileCoreStore = create<MobileCoreState>()(
  persist(
    (set, get) => ({
      identityStatus: "idle",
      peerId: null,
      runtimeState: "stopped",
      networkStatus: null,
      devices: [],
      pairedDevicesCache: [],
      selectedFiles: [],
      error: null,
      initialized: false,
      startedAt: null,

      async loadIdentity() {
        if (get().initialized) return;
        set({ error: null, identityStatus: "loading" });
        try {
          const core = await initMobileCore();
          const identity = await core.initializeIdentity();
          set({
            peerId: identity.peerId,
            identityStatus: "ready",
            initialized: true,
          });
          // 仅在用户开启「自动启动节点」时冷启动一次,后续用户手动停止不会被这里重启。
          if (usePreferencesStore.getState().autoStart) {
            await get().startNode();
          }
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
            identityStatus: "failed",
          });
        }
      },

      async shutdownNode() {
        try {
          const core = await initMobileCore();
          await core.shutdownNode();
          set({
            runtimeState: "stopped",
            networkStatus: null,
            startedAt: null,
            devices: [],
          });
        } catch (err) {
          console.warn("[mobile-core-store] shutdownNode failed:", err);
        }
      },

      async startNode() {
        if (
          get().runtimeState === "running" ||
          get().runtimeState === "starting"
        )
          return;
        set({ runtimeState: "starting", error: null });
        try {
          const core = await initMobileCore();
          await core.startNode(
            usePreferencesStore.getState().customBootstrapNodes,
          );
          const networkStatus = await core.networkStatus();
          const devices = await core.listDevices("all");
          const nextRuntimeState = toRuntimeState(networkStatus.status);
          set({
            networkStatus,
            devices,
            pairedDevicesCache: toPairedSummaries(devices),
            runtimeState: nextRuntimeState,
            startedAt: nextRuntimeState === "running" ? Date.now() : null,
          });
        } catch (err) {
          set({
            error: err instanceof Error ? err.message : String(err),
            runtimeState: "error",
            startedAt: null,
          });
        }
      },

      async refreshDevices() {
        try {
          const core = await initMobileCore();
          const devices = await core.listDevices("all");
          const networkStatus = await core.networkStatus();
          set({
            devices,
            pairedDevicesCache: toPairedSummaries(devices),
            networkStatus,
            runtimeState: toRuntimeState(networkStatus.status),
          });
        } catch (err) {
          set({ error: err instanceof Error ? err.message : String(err) });
        }
      },

      async refreshNetworkStatus() {
        try {
          const core = await initMobileCore();
          const networkStatus = await core.networkStatus();
          set({
            networkStatus,
            runtimeState: toRuntimeState(networkStatus.status),
          });
        } catch (err) {
          set({ error: err instanceof Error ? err.message : String(err) });
        }
      },

      async chooseFiles() {
        try {
          const files = await pickTransferFiles();
          set({ selectedFiles: files });
        } catch (err) {
          set({ error: err instanceof Error ? err.message : String(err) });
        }
      },

      setSelectedFiles(files) {
        set({ selectedFiles: files });
      },

      clearSelectedFiles() {
        set({ selectedFiles: [] });
      },

      applyNetworkStatus(status) {
        set({
          networkStatus: status,
          runtimeState: toRuntimeState(status.status),
        });
      },

      applyDevices(devices) {
        set({
          devices,
          pairedDevicesCache: toPairedSummaries(devices),
        });
      },

      setError(error) {
        set({ error });
      },
    }),
    {
      name: "swarmdrop-mobile-core",
      storage: createJSONStorage(() => AsyncStorage),
      // 只持久化"上次拿到的已配对设备骨架",其余字段都是运行时态
      partialize: (state) => ({
        pairedDevicesCache: state.pairedDevicesCache,
      }),
    },
  ),
);

/** 把 `PairedDeviceSummary` 还原为 `DeviceInfo` 形态(status="offline"),给主屏 fallback 渲染。 */
export function summariesToOfflineDevices(
  summaries: PairedDeviceSummary[],
): DeviceInfo[] {
  return summaries.map((s) => ({
    peerId: s.peerId,
    hostname: s.hostname,
    os: s.os,
    platform: s.platform,
    arch: s.arch,
    status: "offline",
    connection: undefined,
    latencyMs: undefined,
    isPaired: true,
  }));
}
