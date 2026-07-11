/**
 * Preferences Store
 * 管理用户偏好设置（主题、语言、设备名称等）
 * 使用 tauri-plugin-store 持久化到应用配置目录，无需加密保护
 */

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { createTauriStorage } from "@/lib/tauri-store";
import { dynamicActivate, defaultLocale, type LocaleKey } from "@/lib/i18n";
import { commands } from "@/lib/bindings";
import {
  emptyDeviceOrganization,
  type DeviceOrganization,
} from "@/lib/device-organization";
import type {
  FileBrowserScope,
  FileBrowserView,
} from "@/components/file-browser";

export type DiscoveryMode = "auto" | "lanOnly";

/** 点窗口 ✕ 时的行为：每次询问 / 最小化到托盘 / 退出应用。 */
export type CloseBehavior = "ask" | "tray" | "quit";

export function normalizeDeviceOrganization(value: unknown): DeviceOrganization {
  if (!value || typeof value !== "object") {
    return { aliases: {}, groups: [], groupDeviceIds: {} };
  }

  const source = value as Record<string, unknown>;
  const groups = Array.isArray(source.groups)
    ? source.groups.flatMap((group, sortOrder) => {
      if (!group || typeof group !== "object") return [];
      const candidate = group as Record<string, unknown>;
      if (typeof candidate.id !== "string" || typeof candidate.name !== "string") {
        return [];
      }
      return [{
        id: candidate.id,
        name: candidate.name,
        sortOrder: typeof candidate.sortOrder === "number"
          ? candidate.sortOrder
          : sortOrder,
      }];
    })
    : [];
  const groupIds = new Set(groups.map((group) => group.id));
  const aliases = Object.fromEntries(
    Object.entries(source.aliases ?? {}).filter(
      ([, alias]) => typeof alias === "string" && alias.trim(),
    ),
  ) as Record<string, string>;
  const groupDeviceIds = Object.fromEntries(
    Object.entries(source.groupDeviceIds ?? {})
      .filter(([groupId]) => groupIds.has(groupId))
      .map(([groupId, peerIds]) => [
        groupId,
        Array.isArray(peerIds)
          ? peerIds.filter((peerId): peerId is string => typeof peerId === "string")
          : [],
      ]),
  ) as Record<string, string[]>;

  return { aliases, groups, groupDeviceIds };
}

interface PreferencesState {
  /** 语言 */
  locale: LocaleKey;
  /** 自定义设备名称（为空时使用系统主机名） */
  deviceName: string;
  /** 本机对已配对设备的别名与分组，不同步到对端。 */
  deviceOrganization: DeviceOrganization;
  /** 解锁后自动启动 P2P 节点 */
  autoStart: boolean;
  /** 自定义引导节点地址列表（Multiaddr 格式） */
  customBootstrapNodes: string[];
  /** 网络发现模式 */
  discoveryMode: DiscoveryMode;
  /** 自动发现局域网协助节点 */
  autoDiscoverLanHelpers: boolean;
  /** 本设备提供局域网协助能力 */
  provideLanHelper: boolean;
  /** 公网可达性：允许通过公网中继被跨网设备访问（关闭 = 严格局域网） */
  publicReachability: boolean;
  /** 文件传输设置 */
  transfer: {
    /** 接收文件的默认保存路径 */
    savePath: string;
  };
  /** 各业务场景独立的文件浏览视图偏好。 */
  fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
  /** MCP Server 设置 */
  mcp: {
    /** 监听端口 */
    port: number;
    /** 是否随节点启动自动启动 MCP Server */
    autoStart: boolean;
  };
  /** 点窗口 ✕ 的行为。默认 `ask`：首次询问、可记住。 */
  closeBehavior: CloseBehavior;
  /** 是否已展示过「已最小化到托盘」的首次通知（仅提示一次）。 */
  hasShownTrayHint: boolean;

  // === Actions ===

  /** 设置语言并激活 */
  setLocale: (locale: LocaleKey) => Promise<void>;
  /** 设置设备名称 */
  setDeviceName: (name: string) => void;
  setDeviceAlias: (peerId: string, name: string) => void;
  createDeviceGroup: (name: string) => string | null;
  renameDeviceGroup: (groupId: string, name: string) => void;
  deleteDeviceGroup: (groupId: string) => void;
  reorderDeviceGroups: (groupIds: string[]) => void;
  setDeviceGroups: (peerId: string, groupIds: string[]) => void;
  clearDeviceOrganization: (peerId: string) => void;
  /** 设置自动启动 */
  setAutoStart: (autoStart: boolean) => void;
  /** 添加自定义引导节点 */
  addBootstrapNode: (addr: string) => void;
  /** 删除自定义引导节点 */
  removeBootstrapNode: (addr: string) => void;
  /** 设置网络发现模式 */
  setDiscoveryMode: (mode: DiscoveryMode) => void;
  /** 设置是否自动发现局域网协助节点 */
  setAutoDiscoverLanHelpers: (enabled: boolean) => void;
  /** 设置本设备是否提供局域网协助能力 */
  setProvideLanHelper: (enabled: boolean) => void;
  /** 设置公网可达性 */
  setPublicReachability: (enabled: boolean) => void;
  /** 设置传输保存路径 */
  setTransferSavePath: (path: string) => void;
  /** 设置指定场景的文件浏览视图。 */
  setFileBrowserView: (scope: FileBrowserScope, view: FileBrowserView) => void;
  /** 设置 MCP 端口 */
  setMcpPort: (port: number) => void;
  /** 设置 MCP 自动启动 */
  setMcpAutoStart: (autoStart: boolean) => void;
  /** 设置关闭行为 */
  setCloseBehavior: (behavior: CloseBehavior) => void;
  /** 标记已展示过托盘首次通知 */
  setHasShownTrayHint: (value: boolean) => void;
}

/**
 * 等待偏好设置 hydration 完成
 * 在 main.tsx 初始化时调用，确保主题/语言在渲染前就绑定
 */
export function waitForPreferencesHydration(): Promise<void> {
  return new Promise((resolve) => {
    if (usePreferencesStore.persist.hasHydrated()) {
      resolve();
    } else {
      usePreferencesStore.persist.onFinishHydration(() => resolve());
    }
  });
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      locale: defaultLocale,
      deviceName: "",
      deviceOrganization: emptyDeviceOrganization,
      autoStart: false,
      customBootstrapNodes: [],
      discoveryMode: "auto",
      autoDiscoverLanHelpers: true,
      provideLanHelper: false,
      publicReachability: true,
      transfer: {
        savePath: "",
      },
      fileBrowserViews: {
        send: "tree",
        inbox: "grid",
        transfer: "tree",
      },
      mcp: {
        port: 19527,
        autoStart: false,
      },
      closeBehavior: "ask",
      hasShownTrayHint: false,

      async setLocale(locale: LocaleKey) {
        await dynamicActivate(locale);
        set({ locale });
        // 同步给后端：托盘菜单 / 系统通知等原生字符串随之切换语言并即时重绘托盘。
        // best-effort——后端未就绪 / IPC 失败不影响前端语言已切换。
        try {
          await commands.setLocale(locale);
        } catch {
          // 忽略：前端语言已切换，后端原生字符串下次会话或下次切换时对齐。
        }
      },

      setDeviceName(name: string) {
        set({ deviceName: name });
      },

      setDeviceAlias(peerId, name) {
        const alias = name.trim();
        set((state) => {
          const aliases = { ...state.deviceOrganization.aliases };
          if (alias) aliases[peerId] = alias;
          else delete aliases[peerId];
          return { deviceOrganization: { ...state.deviceOrganization, aliases } };
        });
      },

      createDeviceGroup(name) {
        const groupName = name.trim();
        if (!groupName) return null;
        const id = crypto.randomUUID();
        set((state) => ({
          deviceOrganization: {
            ...state.deviceOrganization,
            groups: [
              ...state.deviceOrganization.groups,
              { id, name: groupName, sortOrder: state.deviceOrganization.groups.length },
            ],
          },
        }));
        return id;
      },

      renameDeviceGroup(groupId, name) {
        const groupName = name.trim();
        if (!groupName) return;
        set((state) => ({
          deviceOrganization: {
            ...state.deviceOrganization,
            groups: state.deviceOrganization.groups.map((group) =>
              group.id === groupId ? { ...group, name: groupName } : group,
            ),
          },
        }));
      },

      deleteDeviceGroup(groupId) {
        set((state) => {
          const groupDeviceIds = { ...state.deviceOrganization.groupDeviceIds };
          delete groupDeviceIds[groupId];
          return {
            deviceOrganization: {
              ...state.deviceOrganization,
              // groups 数组保持插入序，sortOrder 才是用户自定义顺序的载体；删组后
              // 必须先按 sortOrder 排序再重新编号，否则会把 reorder 过的顺序退回
              // 插入序（丢失用户排序）。
              groups: state.deviceOrganization.groups
                .filter((group) => group.id !== groupId)
                .sort((a, b) => a.sortOrder - b.sortOrder)
                .map((group, sortOrder) => ({ ...group, sortOrder })),
              groupDeviceIds,
            },
          };
        });
      },

      reorderDeviceGroups(groupIds) {
        set((state) => {
          const order = new Map(groupIds.map((id, index) => [id, index]));
          return {
            deviceOrganization: {
              ...state.deviceOrganization,
              groups: state.deviceOrganization.groups.map((group) => ({
                ...group,
                sortOrder: order.get(group.id) ?? group.sortOrder,
              })),
            },
          };
        });
      },

      setDeviceGroups(peerId, groupIds) {
        set((state) => {
          const validIds = new Set(state.deviceOrganization.groups.map((group) => group.id));
          const selectedIds = new Set(groupIds.filter((id) => validIds.has(id)));
          const groupDeviceIds = Object.fromEntries(
            state.deviceOrganization.groups.map(({ id: groupId }) => {
              const deviceIds = state.deviceOrganization.groupDeviceIds[groupId] ?? [];
              const withoutPeer = deviceIds.filter((id) => id !== peerId);
              return [
                groupId,
                selectedIds.has(groupId) ? [...withoutPeer, peerId] : withoutPeer,
              ];
            }),
          );
          return { deviceOrganization: { ...state.deviceOrganization, groupDeviceIds } };
        });
      },

      clearDeviceOrganization(peerId) {
        set((state) => {
          const aliases = { ...state.deviceOrganization.aliases };
          delete aliases[peerId];
          return {
            deviceOrganization: {
              ...state.deviceOrganization,
              aliases,
              groupDeviceIds: Object.fromEntries(
                Object.entries(state.deviceOrganization.groupDeviceIds).map(([groupId, deviceIds]) => [
                  groupId,
                  deviceIds.filter((id) => id !== peerId),
                ]),
              ),
            },
          };
        });
      },

      setAutoStart(autoStart: boolean) {
        set({ autoStart });
      },

      addBootstrapNode(addr: string) {
        set((state) => ({
          customBootstrapNodes: [...state.customBootstrapNodes, addr],
        }));
      },

      removeBootstrapNode(addr: string) {
        set((state) => ({
          customBootstrapNodes: state.customBootstrapNodes.filter((n) => n !== addr),
        }));
      },

      setDiscoveryMode(discoveryMode: DiscoveryMode) {
        set({ discoveryMode });
      },

      setAutoDiscoverLanHelpers(autoDiscoverLanHelpers: boolean) {
        set({ autoDiscoverLanHelpers });
      },

      setProvideLanHelper(provideLanHelper: boolean) {
        set({ provideLanHelper });
      },

      setPublicReachability(publicReachability: boolean) {
        set({ publicReachability });
      },

      setTransferSavePath(path: string) {
        set((state) => ({
          transfer: { ...state.transfer, savePath: path },
        }));
      },

      setFileBrowserView(scope, view) {
        set((state) => ({
          fileBrowserViews: { ...state.fileBrowserViews, [scope]: view },
        }));
      },

      setMcpPort(port: number) {
        set((state) => ({
          mcp: { ...state.mcp, port },
        }));
      },

      setMcpAutoStart(autoStart: boolean) {
        set((state) => ({
          mcp: { ...state.mcp, autoStart },
        }));
      },

      setCloseBehavior(behavior: CloseBehavior) {
        set({ closeBehavior: behavior });
      },

      setHasShownTrayHint(value: boolean) {
        set({ hasShownTrayHint: value });
      },
    }),
    {
      name: "preferences-store",
      version: 1,
      storage: createJSONStorage(() => createTauriStorage("preferences.json")),
      migrate: (persistedState) => {
        const persisted = persistedState as Partial<PreferencesState>;
        return {
          ...persisted,
          deviceOrganization: normalizeDeviceOrganization(
            persisted.deviceOrganization,
          ),
        };
      },
      merge: (persistedState, currentState) => {
        const persisted = persistedState as Partial<PreferencesState>;
        return {
          ...currentState,
          ...persisted,
          deviceOrganization: normalizeDeviceOrganization(
            persisted.deviceOrganization,
          ),
        };
      },
      partialize: (state) => ({
        locale: state.locale,
        deviceName: state.deviceName,
        deviceOrganization: state.deviceOrganization,
        autoStart: state.autoStart,
        customBootstrapNodes: state.customBootstrapNodes,
        discoveryMode: state.discoveryMode,
        autoDiscoverLanHelpers: state.autoDiscoverLanHelpers,
        provideLanHelper: state.provideLanHelper,
        publicReachability: state.publicReachability,
        transfer: state.transfer,
        fileBrowserViews: state.fileBrowserViews,
        mcp: state.mcp,
        closeBehavior: state.closeBehavior,
        hasShownTrayHint: state.hasShownTrayHint,
      }),
      onRehydrateStorage: () => {
        return (state) => {
          if (state) {
            // hydration 完成后立即激活语言
            dynamicActivate(state.locale);
          }
        };
      },
    }
  )
);
