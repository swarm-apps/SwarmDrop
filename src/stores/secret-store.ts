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
import { commands, events, type PairedDeviceInfo } from "@/lib/bindings";
import { getErrorMessage } from "@/lib/errors";

/** 已配对设备信息（直接复用后端 specta 生成类型）。 */
export type PairedDevice = PairedDeviceInfo;

interface SecretState {
  /** protobuf 编码的密钥对，仅作为运行时镜像 */
  keypair: number[] | null;
  /** 设备 ID (PeerId) */
  deviceId: string | null;
  /** 已配对设备列表 */
  pairedDevices: PairedDevice[];
  /** 身份初始化错误（如 keychain 访问被拒），null 表示成功 */
  initError: string | null;
  /** 是否已完成 hydration */
  _hasHydrated: boolean;

  setHasHydrated: (state: boolean) => void;
  init: () => Promise<void>;
  addPairedDevice: (device: Omit<PairedDevice, "pairedAt">) => void;
  upsertPairedDevice: (device: PairedDevice) => void;
  removePairedDevice: (peerId: string) => void;
  updatePairedDeviceHostname: (peerId: string, hostname: string) => void;
}

export const useSecretStore = create<SecretState>()((set, get) => ({
  keypair: null,
  deviceId: null,
  pairedDevices: [],
  initError: null,
  _hasHydrated: false,

  setHasHydrated: (state) => set({ _hasHydrated: state }),

  async init() {
    try {
      const identity = await commands.initializeIdentity();
      set({
        keypair: identity.keypair,
        deviceId: identity.deviceId,
        pairedDevices: identity.pairedDevices,
        initError: null,
      });
    } catch (err) {
      // 身份初始化失败（如 dev 二进制签名漂移导致 keychain 拒读）不应让整个应用
      // 启动 reject 成静默 UNHANDLED_REJECTION——记录错误，供 startNetwork 等流程
      // 给出明确提示，而不是表现为"点了没反应"。
      console.error("Failed to initialize identity:", err);
      set({ deviceId: null, initError: getErrorMessage(err) });
    }
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

  upsertPairedDevice(device) {
    const pairedDevices = get().pairedDevices;
    const exists = pairedDevices.some((d) => d.peerId === device.peerId);
    set({
      pairedDevices: exists
        ? pairedDevices.map((d) => (d.peerId === device.peerId ? device : d))
        : [...pairedDevices, device],
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

let unlistenPairedDeviceRemoved: (() => void) | null = null;

/**
 * 订阅「已解除配对」事件（后端已完成持久化删除）。
 *
 * 这是前端移除该设备的**唯一**入口：调用 `removePairedDevice` 命令的地方不再顺手改
 * 本地状态，否则同一条记录会被两条路径各删一次。
 *
 * **挂在应用生命周期上而不是节点生命周期上**：解除配对在节点停止时同样可用
 * （后端那条路径直接删持久化并补发本事件），而 network-store 的监听器会随
 * `stopNetwork` 一起注销——挂在那里的话，节点停着时解除就变成「点了没反应」。
 */
export async function setupSecretListeners() {
  cleanupSecretListeners();
  unlistenPairedDeviceRemoved = await events.pairedDeviceRemoved.listen((event) => {
    useSecretStore.getState().removePairedDevice(event.payload);
  });
}

export function cleanupSecretListeners() {
  unlistenPairedDeviceRemoved?.();
  unlistenPairedDeviceRemoved = null;
}
