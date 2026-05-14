import type {
  MobileDevice as DeviceInfo,
  MobileNetworkStatus as NetworkStatus,
  MobileTransferFile as TransferFile,
} from "react-native-swarmdrop-core";
import { create } from "zustand";
import { pickTransferFiles } from "@/core/file-access";
import { initMobileCore } from "@/core/mobile-core";

export type RuntimeState = "stopped" | "starting" | "running" | "error";

// Rust 端 networkStatus.status 是任意字符串,RN 用 union 类型,需要收敛
function toRuntimeState(status: string): RuntimeState {
  return status === "running" ? "running" : "stopped";
}

type MobileCoreState = {
  identityStatus: string;
  peerId: string | null;
  runtimeState: RuntimeState;
  networkStatus: NetworkStatus | null;
  devices: DeviceInfo[];
  selectedFiles: TransferFile[];
  error: string | null;
  initialized: boolean;
  initialize: () => Promise<void>;
  refreshDevices: () => Promise<void>;
  refreshNetworkStatus: () => Promise<void>;
  chooseFiles: () => Promise<void>;
  clearSelectedFiles: () => void;
  applyNetworkStatus: (status: NetworkStatus) => void;
  applyDevices: (devices: DeviceInfo[]) => void;
  setError: (error: string | null) => void;
};

export const useMobileCoreStore = create<MobileCoreState>((set, get) => ({
  identityStatus: "等待初始化",
  peerId: null,
  runtimeState: "stopped",
  networkStatus: null,
  devices: [],
  selectedFiles: [],
  error: null,
  initialized: false,

  async initialize() {
    if (get().initialized && get().runtimeState === "running") return;
    set({
      error: null,
      identityStatus: "正在初始化设备身份",
      runtimeState: "starting",
    });
    try {
      const core = await initMobileCore();
      const identity = await core.initializeIdentity();
      set({
        peerId: identity.peerId,
        identityStatus: `已加载身份 ${identity.peerId.slice(0, 12)}...`,
      });
      await core.startNode([]);
      const networkStatus = await core.networkStatus();
      const devices = await core.listDevices("all");
      set({
        networkStatus,
        devices,
        runtimeState: toRuntimeState(networkStatus.status),
        initialized: true,
      });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        runtimeState: "error",
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
}));
