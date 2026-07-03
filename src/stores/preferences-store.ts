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

export type DiscoveryMode = "auto" | "lanOnly";

/** 点窗口 ✕ 时的行为：每次询问 / 最小化到托盘 / 退出应用。 */
export type CloseBehavior = "ask" | "tray" | "quit";

interface PreferencesState {
  /** 语言 */
  locale: LocaleKey;
  /** 自定义设备名称（为空时使用系统主机名） */
  deviceName: string;
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
  /** 文件传输设置 */
  transfer: {
    /** 接收文件的默认保存路径 */
    savePath: string;
  };
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
  /** 设置传输保存路径 */
  setTransferSavePath: (path: string) => void;
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
      autoStart: false,
      customBootstrapNodes: [],
      discoveryMode: "auto",
      autoDiscoverLanHelpers: true,
      provideLanHelper: false,
      transfer: {
        savePath: "",
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

      setTransferSavePath(path: string) {
        set((state) => ({
          transfer: { ...state.transfer, savePath: path },
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
      storage: createJSONStorage(() => createTauriStorage("preferences.json")),
      partialize: (state) => ({
        locale: state.locale,
        deviceName: state.deviceName,
        autoStart: state.autoStart,
        customBootstrapNodes: state.customBootstrapNodes,
        discoveryMode: state.discoveryMode,
        autoDiscoverLanHelpers: state.autoDiscoverLanHelpers,
        provideLanHelper: state.provideLanHelper,
        transfer: state.transfer,
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
