import AsyncStorage from "@react-native-async-storage/async-storage";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

interface PreferencesState {
  /** 用户自定义设备名,空字符串走系统 hostname / Device.deviceName fallback */
  deviceName: string;
  /** App 启动时是否自动启动 P2P 节点,默认 false(对齐桌面 autoStart) */
  autoStart: boolean;
  /** 自定义引导节点(Multiaddr 字符串数组),与后端 DEFAULT_BOOTSTRAP_NODES 合并 */
  customBootstrapNodes: string[];
  setDeviceName: (name: string) => void;
  setAutoStart: (value: boolean) => void;
  addBootstrapNode: (addr: string) => void;
  removeBootstrapNode: (addr: string) => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      deviceName: "",
      autoStart: false,
      customBootstrapNodes: [],

      setDeviceName(name) {
        set({ deviceName: name.trim() });
      },

      setAutoStart(value) {
        set({ autoStart: value });
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
    }),
    {
      name: "swarmdrop-preferences",
      storage: createJSONStorage(() => AsyncStorage),
    },
  ),
);
