/**
 * Network Store
 * 管理 P2P 网络状态，消费后端数据
 */

import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Device, NetworkStatus } from "@/commands/network";
import {
  start,
  shutdown,
  listDevices,
  getNetworkStatus,
} from "@/commands/network";
import { startMcpServer } from "@/commands/mcp";
import {
  DEVICES_CHANGED,
  NETWORK_STATUS_CHANGED,
  PAIRING_REQUEST_RECEIVED,
  PAIRED_DEVICE_ADDED,
} from "@/constants/events";
import { getErrorMessage } from "@/lib/errors";
import { useSecretStore, type PairedDevice } from "@/stores/secret-store";
import { usePairingStore } from "@/stores/pairing-store";
import { usePreferencesStore } from "@/stores/preferences-store";

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

  // === Actions ===

  /** 启动网络 */
  startNetwork: () => Promise<void>;
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
  /** 清除错误 */
  clearError: () => void;
}

// Tauri Event 监听器清理函数
let unlistenFns: UnlistenFn[] = [];

/** 设置 Tauri Event 监听（直接接收后端推送的 payload） */
async function setupEventListeners() {
  // 清理旧的监听器
  await cleanupEventListeners();

  const fns = await Promise.all([
    // 设备列表变更（后端推送完整列表）
    listen<Device[]>(DEVICES_CHANGED, (event) => {
      useNetworkStore.setState({ devices: event.payload });
    }),

    // 网络状态变更（后端推送完整状态，同时判断节点是否已启动）
    listen<NetworkStatus>(NETWORK_STATUS_CHANGED, (event) => {
      const store = useNetworkStore.getState();
      const updates: Partial<NetworkState> = { networkStatus: event.payload };
      if (event.payload.status === "running" && store.status !== "running") {
        updates.status = "running";
        updates.startedAt = store.startedAt ?? Date.now();
      }
      useNetworkStore.setState(updates);
    }),

    // 配对请求（转发给 pairing-store）
    listen(PAIRING_REQUEST_RECEIVED, (event) => {
      usePairingStore.getState().handleInboundRequest(event.payload as any);
    }),

    // 配对成功（后端已添加到运行时，并写入 host keychain 持久化）
    listen<PairedDevice>(PAIRED_DEVICE_ADDED, (event) => {
      useSecretStore.getState().addPairedDevice(event.payload);
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

  async startNetwork() {
    const { status } = get();
    if (status === "running" || status === "starting") return;

    // 检查 keypair 是否已初始化
    const { deviceId, pairedDevices } = useSecretStore.getState();
    if (!deviceId) {
      set({ status: "error", error: "Keypair not initialized" });
      return;
    }

    set({
      status: "starting",
      error: null,
      devices: [],
      networkStatus: null,
    });

    try {
      // 设置 Tauri Event 监听（在启动前设置，避免丢失早期事件）
      await setupEventListeners();

      const { customBootstrapNodes, mcp } = usePreferencesStore.getState();
      await start(pairedDevices, customBootstrapNodes);

      // 如果启用了 MCP 自动启动，启动 MCP Server
      if (mcp.autoStart) {
        startMcpServer(mcp.port).catch((err) => {
          console.error("Failed to auto-start MCP server:", err);
        });
      }

      // status 会在收到 listening 事件后更新为 running
    } catch (err) {
      console.error("Failed to start node:", err);
      await cleanupEventListeners();
      set({
        status: "error",
        error: getErrorMessage(err),
      });
    }
  },

  async stopNetwork() {
    const { status } = get();
    if (status !== "running") return;

    try {
      await shutdown();
      await cleanupEventListeners();
      usePairingStore.getState().reset();
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
      const result = await listDevices(filter);
      set({ devices: result.devices });
    } catch (err) {
      console.error("Failed to fetch devices:", err);
    }
  },

  async fetchNetworkStatus() {
    try {
      const status = await getNetworkStatus();
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

  clearError() {
    set({ error: null });
  },
}));
