/**
 * Secret Store
 * 设备身份由后端通过系统 keychain 管理，前端只保留运行时镜像。
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import { initializeIdentity } from "@/commands/identity";

/** 已配对设备信息（与后端 PairedDeviceInfo 对齐）。 */
export interface PairedDevice {
  /** PeerId */
  peerId: string;
  /** 设备主机名 */
  hostname: string;
  /** 操作系统类型 */
  os: string;
  /** 平台 */
  platform: string;
  /** 架构 */
  arch: string;
  /** 配对时间戳 */
  pairedAt: number;
}

interface SecretState {
  /** protobuf 编码的密钥对，仅作为运行时镜像 */
  keypair: number[] | null;
  /** 设备 ID (PeerId) */
  deviceId: string | null;
  /** 已配对设备列表 */
  pairedDevices: PairedDevice[];
  /** 是否已完成 hydration */
  _hasHydrated: boolean;

  setHasHydrated: (state: boolean) => void;
  init: () => Promise<void>;
  addPairedDevice: (device: Omit<PairedDevice, "pairedAt">) => void;
  removePairedDevice: (peerId: string) => void;
  updatePairedDeviceHostname: (peerId: string, hostname: string) => void;
}

export const useSecretStore = create<SecretState>()(
  persist(
    (set, get) => ({
      keypair: null,
      deviceId: null,
      pairedDevices: [],
      _hasHydrated: false,

      setHasHydrated: (state) => set({ _hasHydrated: state }),

      async init() {
        const identity = await initializeIdentity();
        set({
          keypair: identity.keypair,
          deviceId: identity.deviceId,
          pairedDevices: identity.pairedDevices,
        });
      },

      addPairedDevice(device) {
        const { pairedDevices } = get();
        if (pairedDevices.some((d) => d.peerId === device.peerId)) {
          return;
        }
        set({
          pairedDevices: [
            ...pairedDevices,
            { ...device, pairedAt: Date.now() },
          ],
        });
      },

      removePairedDevice(peerId) {
        set({
          pairedDevices: get().pairedDevices.filter((d) => d.peerId !== peerId),
        });
      },

      updatePairedDeviceHostname(peerId: string, hostname: string) {
        set({
          pairedDevices: get().pairedDevices.map((d) =>
            d.peerId === peerId ? { ...d, hostname } : d,
          ),
        });
      },
    }),
    {
      name: "secret-store",
      // 不再把密钥或配对设备写入前端持久化存储；启动时统一从 host keychain 读取。
      partialize: () => ({}),
    },
  ),
);

/**
 * 初始化设备身份运行时镜像。
 */
export async function rehydrateSecretStore() {
  await useSecretStore.persist.rehydrate();
  const state = useSecretStore.getState();
  await state.init();
  state.setHasHydrated(true);
}
