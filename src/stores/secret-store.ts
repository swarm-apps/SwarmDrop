/**
 * Secret Store
 *
 * 设备身份和已配对设备列表的运行时镜像。
 *
 * **不再持久化到 localStorage**——身份由后端 host keychain 管理，每次启动从
 * `initializeIdentity()` 重新读取。原先包了一层 zustand `persist` middleware
 * 但 `partialize: () => ({})` 不写任何字段，是名存实亡的空壳，已删除。
 */

import { create } from "zustand";
import { commands, type PairedDeviceInfo } from "@/lib/bindings";

/** 已配对设备信息（直接复用后端 specta 生成类型）。 */
export type PairedDevice = PairedDeviceInfo;

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

export const useSecretStore = create<SecretState>()((set, get) => ({
  keypair: null,
  deviceId: null,
  pairedDevices: [],
  _hasHydrated: false,

  setHasHydrated: (state) => set({ _hasHydrated: state }),

  async init() {
    const identity = await commands.initializeIdentity();
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
}));

/**
 * 初始化设备身份运行时镜像（从后端 host keychain 拉取）。
 */
export async function rehydrateSecretStore() {
  const state = useSecretStore.getState();
  await state.init();
  state.setHasHydrated(true);
}
