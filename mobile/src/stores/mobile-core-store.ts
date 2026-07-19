import AsyncStorage from "@react-native-async-storage/async-storage";
import type {
  MobileDevice as DeviceInfo,
  MobileDeviceReceivePolicy,
  MobileDeviceTrustLevel,
  MobileNetworkStatus as NetworkStatus,
  MobileTransferFile as TransferFile,
} from "react-native-swarmdrop-core";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import {
  removeSelectedDirectory as filterSelectedDirectory,
  mergeSelectedFiles,
  removeSelectedFile,
} from "@/core/file-browser-identity";
import {
  startForegroundKeepAlive,
  stopForegroundKeepAlive,
} from "@/core/foreground-service";
import { initMobileCore } from "@/core/mobile-core";
import { buildNetworkRuntimeConfig } from "@/core/network-discovery";
import { ensureNotificationPermission } from "@/core/notifier";
import { errorMessage } from "@/lib/utils";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export type RuntimeState = "stopped" | "starting" | "running" | "error";
/** 设备身份加载状态 —— UI 端用 i18n 渲染对应文案 */
export type IdentityStatus = "idle" | "loading" | "ready" | "failed";
export type NodeCommandResult =
  | { ok: true; state: RuntimeState }
  | { ok: false; state: RuntimeState; error: string };

/** 已配对设备的骨架信息 —— 节点未启动时也能显示 */
export interface PairedDeviceSummary {
  peerId: string;
  /** 对端用户起的设备名；持久化里可能没有（老数据），UI 用 name ?? hostname */
  name?: string | null;
  hostname: string;
  os: string;
  platform: string;
  arch: string;
  trustLevel?: DeviceInfo["trustLevel"] | null;
  receivePolicy?: DeviceInfo["receivePolicy"] | null;
  trustConfirmed?: boolean | null;
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
      name: d.name,
      hostname: d.hostname,
      os: d.os,
      platform: d.platform,
      arch: d.arch,
      trustLevel: d.trustLevel,
      receivePolicy: d.receivePolicy,
      trustConfirmed: d.trustConfirmed,
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
  /** 直接从 Rust keychain 读已配对设备,刷新 cache。
   *  不依赖 NetManager,节点未启动也能调,UI 离线视图的权威数据源。 */
  loadPairedDevicesCache: () => Promise<void>;
  /** 关闭 NetManager 释放 P2P 资源（用户手动停或 app 即将被杀时调用） */
  shutdownNode: () => Promise<NodeCommandResult>;
  /** 启动 NetManager */
  startNode: () => Promise<NodeCommandResult>;
  refreshDevices: () => Promise<void>;
  refreshNetworkStatus: () => Promise<void>;
  updatePairedDevicePolicy: (
    peerId: string,
    trustLevel: MobileDeviceTrustLevel,
    receivePolicy?: MobileDeviceReceivePolicy,
  ) => Promise<DeviceInfo>;
  removePairedDevice: (peerId: string) => Promise<void>;
  /** 追加文件到当前选择（多次选择叠加，按稳定 sourceId 去重） */
  appendFiles: (files: TransferFile[]) => void;
  removeSelectedBySourceId: (sourceId: string) => void;
  removeSelectedDirectory: (relativeDirectory: string) => void;
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
          // 身份就绪后立即拉一次 keychain paired 设备,
          // 保证主屏/选择接收设备页冷启动就有离线视图(不依赖节点是否启动)。
          await get().loadPairedDevicesCache();
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

      async loadPairedDevicesCache() {
        try {
          const core = await initMobileCore();
          const devices = await core.listPairedDevices();
          set({ pairedDevicesCache: toPairedSummaries(devices) });
        } catch (err) {
          console.warn(
            "[mobile-core-store] loadPairedDevicesCache failed:",
            err,
          );
        }
      },

      async shutdownNode() {
        try {
          const core = await initMobileCore();
          await core.shutdownNode();
          // 节点停 → 拆前台服务(node running ⇔ FGS up)
          void stopForegroundKeepAlive();
          set({
            runtimeState: "stopped",
            networkStatus: null,
            startedAt: null,
            devices: [],
          });
          // 节点停了，配对码也无效（不能通过 DHT 被对端找到）—— 清掉
          usePairingInviteStore.getState().clearInvite();
          return { ok: true, state: "stopped" };
        } catch (err) {
          const message = errorMessage(err);
          console.warn("[mobile-core-store] shutdownNode failed:", err);
          set({ error: message });
          return { ok: false, state: get().runtimeState, error: message };
        }
      },

      async startNode() {
        if (
          get().runtimeState === "running" ||
          get().runtimeState === "starting"
        )
          return { ok: true, state: get().runtimeState };
        set({ runtimeState: "starting", error: null });
        try {
          const core = await initMobileCore();
          const prefs = usePreferencesStore.getState();
          await core.startNode(
            prefs.deviceName?.trim() || undefined,
            buildNetworkRuntimeConfig({
              customBootstrapNodes: prefs.customBootstrapNodes,
              discoveryMode: prefs.discoveryMode,
              autoDiscoverLanHelpers: prefs.autoDiscoverLanHelpers,
              provideLanHelper: prefs.provideLanHelper,
              publicReachability: prefs.publicReachability,
            }),
          );
          const networkStatus = await core.networkStatus();
          const devices = await core.listDevices("all");
          const nextRuntimeState = toRuntimeState(networkStatus.status);
          set({
            networkStatus,
            devices,
            runtimeState: nextRuntimeState,
            startedAt: nextRuntimeState === "running" ? Date.now() : null,
          });
          if (nextRuntimeState === "running") {
            // 上线即预热通知权限(比事件到达时惰性申请更早,不打断配对当下),
            // 并拉起前台服务保活(Android;iOS 内部 no-op)。
            void ensureNotificationPermission();
            void startForegroundKeepAlive();
          }
          if (nextRuntimeState !== "running") {
            const message = "节点未进入运行状态";
            set({ error: message });
            return { ok: false, state: nextRuntimeState, error: message };
          }
          return { ok: true, state: nextRuntimeState };
        } catch (err) {
          const message = errorMessage(err);
          set({
            error: message,
            runtimeState: "error",
            startedAt: null,
          });
          return { ok: false, state: "error", error: message };
        }
      },

      async refreshDevices() {
        try {
          const core = await initMobileCore();
          const devices = await core.listDevices("all");
          const networkStatus = await core.networkStatus();
          set({
            devices,
            networkStatus,
            runtimeState: toRuntimeState(networkStatus.status),
          });
        } catch (err) {
          set({ error: errorMessage(err) });
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
          set({ error: errorMessage(err) });
        }
      },

      async updatePairedDevicePolicy(peerId, trustLevel, receivePolicy) {
        try {
          const core = await initMobileCore();
          const updated = await core.updatePairedDevicePolicy(
            peerId,
            trustLevel,
            receivePolicy,
          );
          set((state) => ({
            devices: state.devices.map((device) =>
              device.peerId === peerId ? { ...device, ...updated } : device,
            ),
          }));
          await get().loadPairedDevicesCache();
          return updated;
        } catch (err) {
          set({ error: errorMessage(err) });
          throw err;
        }
      },

      async removePairedDevice(peerId) {
        try {
          const core = await initMobileCore();
          const devices = await core.removePairedDevice(peerId);
          set((state) => ({
            pairedDevicesCache: toPairedSummaries(devices),
            devices: state.devices.filter((device) => device.peerId !== peerId),
          }));
        } catch (err) {
          set({ error: errorMessage(err) });
          throw err;
        }
      },

      appendFiles(files) {
        if (files.length === 0) return;
        set({ selectedFiles: mergeSelectedFiles(get().selectedFiles, files) });
      },

      removeSelectedBySourceId(sourceId) {
        set((s) => ({
          selectedFiles: removeSelectedFile(s.selectedFiles, sourceId),
        }));
      },

      removeSelectedDirectory(relativeDirectory) {
        set((s) => ({
          selectedFiles: filterSelectedDirectory(
            s.selectedFiles,
            relativeDirectory,
          ),
        }));
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
        set({ devices });
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
    name: s.name ?? undefined,
    hostname: s.hostname,
    os: s.os,
    platform: s.platform,
    arch: s.arch,
    status: "offline",
    connection: undefined,
    latencyMs: undefined,
    isPaired: true,
    trustLevel: s.trustLevel ?? undefined,
    receivePolicy: s.receivePolicy ?? undefined,
    trustConfirmed: s.trustConfirmed ?? undefined,
  }));
}

/** 已配对设备列表 = keychain 持久化清单 + 实时发现结果；实时结果覆盖离线骨架。 */
export function mergePairedDevicesWithCache(
  devices: DeviceInfo[],
  summaries: PairedDeviceSummary[],
): DeviceInfo[] {
  const summaryMap = new Map(
    summaries.map((summary) => [summary.peerId, summary]),
  );
  const merged = new Map<string, DeviceInfo>();
  for (const device of summariesToOfflineDevices(summaries)) {
    merged.set(device.peerId, device);
  }
  for (const device of devices) {
    const summary = summaryMap.get(device.peerId);
    if (summary || device.isPaired) {
      merged.set(device.peerId, {
        ...device,
        isPaired: true,
        trustLevel: device.trustLevel ?? summary?.trustLevel ?? undefined,
        receivePolicy:
          device.receivePolicy ?? summary?.receivePolicy ?? undefined,
        trustConfirmed:
          device.trustConfirmed ?? summary?.trustConfirmed ?? undefined,
      });
    }
  }
  return Array.from(merged.values());
}
