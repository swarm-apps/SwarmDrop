import AsyncStorage from "@react-native-async-storage/async-storage";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { DiscoveryModePreference } from "@/core/network-discovery";

interface PreferencesState {
  /** 用户自定义设备名,空字符串走系统 hostname / Device.deviceName fallback */
  deviceName: string;
  /** App 启动时是否自动启动 P2P 节点,默认 false(对齐桌面 autoStart) */
  autoStart: boolean;
  /** 网络发现模式：auto 使用公网引导 + LAN helper，lanOnly 只依赖局域网/自定义节点 */
  discoveryMode: DiscoveryModePreference;
  /** 是否自动发现局域网协助节点，默认 true，对齐 shared core 默认发现能力 */
  autoDiscoverLanHelpers: boolean;
  /** 是否让本机提供 LAN Helper 能力。移动端默认 false，不作为主入口推广 */
  provideLanHelper: boolean;
  /** 自定义引导节点(Multiaddr 字符串数组),与后端 DEFAULT_BOOTSTRAP_NODES 合并 */
  customBootstrapNodes: string[];
  /** 用户自定义接收文件保存目录的 URI(file:// 或 content://);null 走默认 transfersInboxUri */
  receivePath: string | null;
  /** 自动接受已配对设备的文件请求,默认 false(对齐桌面 transfer.autoAccept) */
  autoAccept: boolean;
  setDeviceName: (name: string) => void;
  setAutoStart: (value: boolean) => void;
  setDiscoveryMode: (mode: DiscoveryModePreference) => void;
  setAutoDiscoverLanHelpers: (value: boolean) => void;
  addBootstrapNode: (addr: string) => void;
  removeBootstrapNode: (addr: string) => void;
  setReceivePath: (uri: string | null) => void;
  setAutoAccept: (value: boolean) => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      deviceName: "",
      autoStart: false,
      discoveryMode: "auto",
      autoDiscoverLanHelpers: true,
      provideLanHelper: false,
      customBootstrapNodes: [],
      receivePath: null,
      autoAccept: false,

      setDeviceName(name) {
        set({ deviceName: name.trim() });
      },

      setAutoStart(value) {
        set({ autoStart: value });
      },

      setDiscoveryMode(value) {
        set({ discoveryMode: value });
      },

      setAutoDiscoverLanHelpers(value) {
        set({ autoDiscoverLanHelpers: value });
      },

      addBootstrapNode(addr) {
        set((s) =>
          s.customBootstrapNodes.includes(addr)
            ? s
            : { customBootstrapNodes: [...s.customBootstrapNodes, addr] },
        );
      },

      removeBootstrapNode(addr) {
        set((s) => ({
          customBootstrapNodes: s.customBootstrapNodes.filter(
            (a) => a !== addr,
          ),
        }));
      },

      setReceivePath(uri) {
        set({ receivePath: uri && uri.length > 0 ? uri : null });
      },

      setAutoAccept(value) {
        set({ autoAccept: value });
      },
    }),
    {
      name: "swarmdrop-preferences",
      storage: createJSONStorage(() => AsyncStorage),
    },
  ),
);
