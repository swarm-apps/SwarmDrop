import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  DEFAULT_FILE_BROWSER_VIEWS,
  type DeviceOrganization,
  emptyDeviceOrganization,
  normalizeDeviceOrganization,
  normalizeFileBrowserViews,
} from "@swarmdrop/shared-view";
import * as Crypto from "expo-crypto";
import { Platform } from "react-native";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type {
  FileBrowserScope,
  FileBrowserView,
} from "@/components/file-browser/types";

interface PreferencesState {
  /**
   * 用户自定义设备名的**显示镜像**,空字符串走系统 hostname / Device.deviceName fallback。
   *
   * 事实源在 Rust 侧的 `device_config.json`(core 的 `DeviceConfig` 端口):节点启动时
   * 由 core 读进本机 `OsInfo`,对端看到的是那一份。这里留一份镜像只为 UI 冷启动时
   * 立刻有值可渲染,写入只有两个来源:`applyDeviceName`(先写 core 再回写镜像),以及
   * core 广播的 `DeviceRenamed` 事件(改名可能由本页之外的地方发起)。
   */
  deviceName: string;
  /** 本机对已配对设备的别名与分组,仅保存在本机,不同步到对端。 */
  deviceOrganization: DeviceOrganization;
  /** App 启动时是否自动启动 P2P 节点,默认 false(对齐桌面 autoStart) */
  autoStart: boolean;
  /** 网络发现模式：auto 使用公网引导 + LAN helper，lanOnly 只依赖局域网/自定义节点 */
  /** 是否自动发现局域网协助节点，默认 true，对齐 shared core 默认发现能力 */
  autoDiscoverLanHelpers: boolean;
  /** 是否让本机提供 LAN Helper 能力。移动端默认 false，不作为主入口推广 */
  provideLanHelper: boolean;
  /** 公网可达性：允许经公网中继被跨网设备访问，默认 true；关闭 = 严格局域网 */
  publicReachability: boolean;
  /** 自定义引导节点(Multiaddr 字符串数组),启动时与移动端默认节点合并 */
  customBootstrapNodes: string[];
  /**
   * Android 的接收目录（用户选定的 SAF tree `content://`）。**没有默认值**——`null` 表示
   * 尚未选择，此时不能接收，引导流程会把用户领到目录选择步骤。曾经的「回退应用私有目录」
   * 已经取消：那个目录自 Android 11 起连系统文件管理器都进不去。
   *
   * iOS 不读这一项（落点恒为 `Documents`）。判据与三态语义都在 `@/core/receive-location`，
   * 不要直接读这个字段做决策。
   */
  receivePath: string | null;
  fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
  setDeviceName: (name: string) => void;
  setAutoStart: (value: boolean) => void;
  setAutoDiscoverLanHelpers: (value: boolean) => void;
  setProvideLanHelper: (value: boolean) => void;
  setPublicReachability: (value: boolean) => void;
  addBootstrapNode: (addr: string) => void;
  removeBootstrapNode: (addr: string) => void;
  /** 只接受一个真实目录：三态模型下「清空」不是一个用户能做的动作（清了就收不了文件）。 */
  setReceivePath: (uri: string) => void;
  setFileBrowserView: (scope: FileBrowserScope, view: FileBrowserView) => void;
  /** 设置 / 清空某设备的本机别名（空串或纯空白清空）。 */
  setDeviceAlias: (peerId: string, name: string) => void;
  /** 创建分组，返回新分组 id；名称为空返回 null。 */
  createDeviceGroup: (name: string) => string | null;
  renameDeviceGroup: (groupId: string, name: string) => void;
  deleteDeviceGroup: (groupId: string) => void;
  /** 按给定顺序重排分组 sortOrder。 */
  reorderDeviceGroups: (groupIds: string[]) => void;
  /** 设置某设备所属的分组集合（覆盖式）。 */
  setDeviceGroups: (peerId: string, groupIds: string[]) => void;
  /** 取消配对时清理该 PeerId 的别名与全部分组成员关系。 */
  clearDeviceOrganization: (peerId: string) => void;
}

type PersistedPreferences = Partial<
  Pick<
    PreferencesState,
    | "deviceName"
    | "deviceOrganization"
    | "autoStart"
    | "autoDiscoverLanHelpers"
    | "provideLanHelper"
    | "publicReachability"
    | "customBootstrapNodes"
    | "receivePath"
    | "fileBrowserViews"
  >
>;

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      deviceName: "",
      deviceOrganization: emptyDeviceOrganization,
      autoStart: false,
      autoDiscoverLanHelpers: true,
      provideLanHelper: false,
      publicReachability: true,
      customBootstrapNodes: [],
      receivePath: null,
      fileBrowserViews: { ...DEFAULT_FILE_BROWSER_VIEWS },

      setDeviceName(name) {
        set({ deviceName: name.trim() });
      },

      setAutoStart(value) {
        set({ autoStart: value });
      },

      setAutoDiscoverLanHelpers(value) {
        set({ autoDiscoverLanHelpers: value });
      },

      setProvideLanHelper(value) {
        set({ provideLanHelper: value });
      },

      setPublicReachability(value) {
        set({ publicReachability: value });
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
        set({ receivePath: uri });
      },

      setFileBrowserView(scope, view) {
        set((state) => ({
          fileBrowserViews: { ...state.fileBrowserViews, [scope]: view },
        }));
      },

      setDeviceAlias(peerId, name) {
        const alias = name.trim();
        set((state) => {
          const aliases = { ...state.deviceOrganization.aliases };
          if (alias) aliases[peerId] = alias;
          else delete aliases[peerId];
          return {
            deviceOrganization: { ...state.deviceOrganization, aliases },
          };
        });
      },

      createDeviceGroup(name) {
        const groupName = name.trim();
        if (!groupName) return null;
        const id = Crypto.randomUUID();
        set((state) => ({
          deviceOrganization: {
            ...state.deviceOrganization,
            groups: [
              ...state.deviceOrganization.groups,
              {
                id,
                name: groupName,
                sortOrder: state.deviceOrganization.groups.length,
              },
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
              // groups 数组保持插入序,sortOrder 才是用户自定义顺序的载体;
              // 删组后必须先按 sortOrder 排序再重新编号,否则会把 reorder 过的
              // 顺序退回插入序(丢失用户排序)。
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
          const validIds = new Set(
            state.deviceOrganization.groups.map((group) => group.id),
          );
          const selectedIds = new Set(
            groupIds.filter((id) => validIds.has(id)),
          );
          const groupDeviceIds = Object.fromEntries(
            state.deviceOrganization.groups.map(({ id: groupId }) => {
              const deviceIds =
                state.deviceOrganization.groupDeviceIds[groupId] ?? [];
              const withoutPeer = deviceIds.filter((id) => id !== peerId);
              return [
                groupId,
                selectedIds.has(groupId)
                  ? [...withoutPeer, peerId]
                  : withoutPeer,
              ];
            }),
          );
          return {
            deviceOrganization: { ...state.deviceOrganization, groupDeviceIds },
          };
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
                Object.entries(state.deviceOrganization.groupDeviceIds).map(
                  ([groupId, deviceIds]) => [
                    groupId,
                    deviceIds.filter((id) => id !== peerId),
                  ],
                ),
              ),
            },
          };
        });
      },
    }),
    {
      name: "swarmdrop-preferences",
      storage: createJSONStorage(() => AsyncStorage),
      partialize: (state) => ({
        deviceName: state.deviceName,
        deviceOrganization: state.deviceOrganization,
        autoStart: state.autoStart,
        autoDiscoverLanHelpers: state.autoDiscoverLanHelpers,
        provideLanHelper: state.provideLanHelper,
        publicReachability: state.publicReachability,
        customBootstrapNodes: state.customBootstrapNodes,
        receivePath: state.receivePath,
        fileBrowserViews: state.fileBrowserViews,
      }),
      merge: (persisted, current) => {
        const stored =
          persisted && typeof persisted === "object"
            ? (persisted as PersistedPreferences)
            : {};
        return {
          ...current,
          deviceOrganization: normalizeDeviceOrganization(
            stored.deviceOrganization,
          ),
          ...(typeof stored.deviceName === "string"
            ? { deviceName: stored.deviceName }
            : {}),
          ...(typeof stored.autoStart === "boolean"
            ? { autoStart: stored.autoStart }
            : {}),
          ...(typeof stored.autoDiscoverLanHelpers === "boolean"
            ? { autoDiscoverLanHelpers: stored.autoDiscoverLanHelpers }
            : {}),
          ...(typeof stored.provideLanHelper === "boolean"
            ? { provideLanHelper: stored.provideLanHelper }
            : {}),
          ...(typeof stored.publicReachability === "boolean"
            ? { publicReachability: stored.publicReachability }
            : {}),
          ...(Array.isArray(stored.customBootstrapNodes)
            ? {
                customBootstrapNodes: stored.customBootstrapNodes.filter(
                  (addr): addr is string => typeof addr === "string",
                ),
              }
            : {}),
          // iOS 不再有「自定义接收目录」这回事（落点恒为 `Documents`，见
          // `@/core/receive-location`），旧版本存下的那条路径已经不可达——不丢掉的话
          // 它会作为一份永远不被读取的幽灵数据留在偏好里。
          ...(Platform.OS !== "ios" &&
          (typeof stored.receivePath === "string" ||
            stored.receivePath === null)
            ? { receivePath: stored.receivePath }
            : {}),
          fileBrowserViews: normalizeFileBrowserViews(stored.fileBrowserViews),
        };
      },
    },
  ),
);

/**
 * 等待偏好从 AsyncStorage 水合。
 *
 * **引导路由必须等它**：完成状态由 `deviceName` / `receivePath` 派生，水合前它们是初始值，
 * 一个早就配置好的用户会被判成「还没引导」并 `<Redirect>` 进引导流程——而 `Redirect` 是
 * 一次性导航，之后水合完也回不来。同一个窗口还会让冷启动的分享意图被当成「未完成设置」
 * 直接丢弃。
 */
export const waitForPreferencesHydration = (): Promise<void> =>
  new Promise((resolve) => {
    if (usePreferencesStore.persist.hasHydrated()) {
      resolve();
      return;
    }
    const unsub = usePreferencesStore.persist.onFinishHydration(() => {
      unsub();
      resolve();
    });
  });
