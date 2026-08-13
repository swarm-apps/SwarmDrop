/**
 * Network Store
 * 管理 P2P 网络状态，消费后端数据
 */

import { create } from "zustand";
import {
  commands,
  events,
  type Device,
  type NetworkStatus,
} from "@/lib/bindings";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { getErrorMessage } from "@/lib/errors";
import { useSecretStore } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { getDesktopBootstrapNodes } from "@/lib/bootstrap-nodes";

/** 节点状态（前端 UI 生命周期） */
export type NodeStatus = "stopped" | "starting" | "running" | "error";

interface NetworkState {
  /** 节点状态 */
  status: NodeStatus;
  /** 后端设备列表 */
  devices: Device[];
  /** 后端网络状态 */
  networkStatus: NetworkStatus | null;
  /** 错误信息 */
  error: string | null;
  /** 节点启动时间戳 */
  startedAt: number | null;
  /**
   * 有设置改过、但要重启节点才生效。
   *
   * 住在 store 而不是设置区的组件 `useState`：它是**跨路由的**事实——用户改完开关
   * 切去设备页再回来，那条提示不该消失，否则他会以为已经生效了。
   */
  needsRestart: boolean;

  // === Actions ===

  /** 启动网络 */
  startNetwork: () => Promise<boolean>;
  /** 停止网络 */
  stopNetwork: () => Promise<void>;
  /** 从后端获取设备列表 */
  fetchDevices: (filter?: "all" | "connected" | "paired") => Promise<void>;
  /** 从后端获取网络状态 */
  fetchNetworkStatus: () => Promise<void>;
  /** 获取已连接的 peer 数量 */
  getConnectedCount: () => number;
  /** 获取已发现的 peer 数量 */
  getDiscoveredCount: () => number;
  /** 标记 / 清除「需重启节点才生效」。 */
  setNeedsRestart: (needsRestart: boolean) => void;
  /** 清除错误 */
  clearError: () => void;
}

// Tauri Event 监听器清理函数（events.xxx.listen 返回 () => void）
let unlistenFns: Array<() => void> = [];

/** 设置 Tauri Event 监听（直接接收后端推送的 payload） */
async function setupEventListeners() {
  // 清理旧的监听器
  await cleanupEventListeners();

  const fns = await Promise.all([
    // 设备列表变更（后端推送完整列表）
    events.devicesChanged.listen((event) => {
      useNetworkStore.setState({ devices: event.payload });
    }),

    // 网络状态变更（后端推送完整状态，同时判断节点是否已启动）
    events.networkStatusChanged.listen((event) => {
      const store = useNetworkStore.getState();
      const updates: Partial<NetworkState> = { networkStatus: event.payload };
      if (event.payload.status === "running" && store.status !== "running") {
        updates.status = "running";
        updates.startedAt = store.startedAt ?? Date.now();
      }
      useNetworkStore.setState(updates);
    }),

    // 配对请求（转发给 pairing-store）
    events.pairingRequestReceived.listen((event) => {
      usePairingStore.getState().handleInboundRequest(event.payload);
    }),

    // 配对成功或 Identify 刷新（后端已写入身份存储持久化）
    events.pairedDeviceAdded.listen((event) => {
      useSecretStore.getState().upsertPairedDevice(event.payload);
    }),
  ]);

  unlistenFns = fns;
}

/** 清理 Tauri Event 监听 */
async function cleanupEventListeners() {
  for (const unlisten of unlistenFns) {
    unlisten();
  }
  unlistenFns = [];
}

export const useNetworkStore = create<NetworkState>()((set, get) => ({
  status: "stopped",
  devices: [],
  networkStatus: null,
  error: null,
  startedAt: null,
  needsRestart: false,

  async startNetwork() {
    const { status } = get();
    if (status === "running" || status === "starting") return true;

    // 检查 keypair 是否已初始化
    const { deviceId, initError } = useSecretStore.getState();
    if (!deviceId) {
      const reason = initError ?? "设备身份未初始化";
      set({ status: "error", error: reason });
      toast.error(t`节点启动失败`, { description: reason });
      return false;
    }

    set({
      status: "starting",
      error: null,
      devices: [],
      networkStatus: null,
      // 这次启动读的就是当前偏好，之前攒下的「需重启」到此清账。
      needsRestart: false,
    });

    try {
      // 设置 Tauri Event 监听（在启动前设置，避免丢失早期事件）
      await setupEventListeners();

      const {
        customBootstrapNodes,
        autoDiscoverLanHelpers,
        provideLanHelper,
        publicReachability,
        mcp,
      } = usePreferencesStore.getState();
      const networkOptions = {
        bootstrapNodes: getDesktopBootstrapNodes(customBootstrapNodes),
        autoDiscoverLanHelpers,
        provideLanHelper,
        publicReachability,
      };
      // 已配对设备不再由前端传入：事实源是后端的 PairedDeviceStore 端口，
      // core 在 start_node 内部自取（前端那份本来就是后端的镜像）。
      await commands.start(networkOptions);

      // 如果启用了 MCP 自动启动，启动 MCP Server
      if (mcp.autoStart) {
        commands.startMcpServer(mcp.port).catch((err) => {
          console.error("Failed to auto-start MCP server:", err);
        });
      }

      // status 会在收到 listening 事件后更新为 running
      return true;
    } catch (err) {
      console.error("Failed to start node:", err);
      await cleanupEventListeners();
      const message = getErrorMessage(err);
      set({
        status: "error",
        error: message,
      });
      toast.error(t`节点启动失败`, { description: message });
      return false;
    }
  },

  async stopNetwork() {
    const { status } = get();
    if (status !== "running") return;

    try {
      await commands.shutdown();
      await cleanupEventListeners();
      usePairingStore.getState().reset();
      usePairingStore.getState().clearActiveInvite();
      set({
        status: "stopped",
        devices: [],
        networkStatus: null,
        startedAt: null,
      });
    } catch (err) {
      console.error("Failed to shutdown node:", err);
      set({ error: getErrorMessage(err) });
    }
  },

  async fetchDevices(filter) {
    try {
      const result = await commands.listDevices(filter ?? null);
      set({ devices: result.devices });
    } catch (err) {
      console.error("Failed to fetch devices:", err);
    }
  },

  async fetchNetworkStatus() {
    try {
      const status = await commands.getNetworkStatus();
      set({ networkStatus: status });
    } catch (err) {
      console.error("Failed to fetch network status:", err);
    }
  },

  getConnectedCount(): number {
    const { networkStatus } = get();
    return networkStatus?.connectedPeers ?? 0;
  },

  getDiscoveredCount(): number {
    const { networkStatus } = get();
    return networkStatus?.discoveredPeers ?? 0;
  },

  setNeedsRestart(needsRestart) {
    set({ needsRestart });
  },

  clearError() {
    set({ error: null });
  },
}));

/** 命令式边界：供非 React/store orchestration 回调启动节点。 */
export function startNetworkFromStore(): Promise<boolean> {
  return useNetworkStore.getState().startNetwork();
}

/**
 * 冷启动时的一次性自动启动：读一次「自动启动节点」偏好，为真则启动节点。
 *
 * **只在冷启动序列里调用一次**（`src/main.tsx`），不订阅任何状态。它此前是 `_app.tsx`
 * 里一个依赖 `networkStatus` 的 effect，于是「App 启动时自动启动一次」被写成了
 * `stopped → running` 的持续收敛环：`stopNetwork()` 的最后一步正是把状态置为 `stopped`，
 * effect 立刻把节点拉回去——开关打开时「已停止」这个状态根本不可达，用户点了停止只看到
 * 状态一闪就回。设置文案（「解锁后自动启动」）与那个 effect 自己的注释（「首次进入时检查」）
 * 表达的都是一次性意图，只有代码是收敛环。
 *
 * 三端至此同一形态：Web 是空依赖 `useEffect`（`web-node-bootstrap.tsx`），
 * 移动端在冷启动序列里命令式读一次（`mobile-core-store.ts`）。
 *
 * 失败不在这里提示：`startNetwork()` 内部已落 error 状态并 toast，与手动启动同一条反馈路径。
 */
export async function autoStartNodeIfEnabled(): Promise<void> {
  if (!usePreferencesStore.getState().autoStart) return;
  // 走 `startNetworkFromStore` 而不是自己 `getState().startNetwork()`：那条正是
  // 「从 React 外面启动节点」的既有边界（`pairing-store` 也走它），两个入口并存意味着
  // 将来往这个边界上加东西（重入门禁、启动来源打点）必须记得两处都加。
  await startNetworkFromStore();
}

/** 命令式边界：供配对流程用当前网络快照补全设备展示名。 */
export function findNetworkDeviceSnapshot(peerId: string): Device | undefined {
  return useNetworkStore.getState().devices.find((device) => device.peerId === peerId);
}
