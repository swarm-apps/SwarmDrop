// Web 应用区的**持久化偏好**。
//
// 与 `store.ts` 刻意分开：那份是运行时节点状态（节点一关就该没了），这份是用户的本机设置
// （关标签页也要留着）。桌面端同样是这个分工——`network-store` 运行时、`preferences-store`
// 持久化——混在一起会让「什么该在刷新后还在」变成一道要逐字段判断的题。
//
// 目前有两样：设备组织（别名 + 分组）与文件浏览器的视图偏好。两者都**不同步给对端**，
// 纯本机显示投影，所以放 localStorage 就够，不必进 IndexedDB。
//
// ## 静态导出下的 hydration
//
// 预渲染发生在构建期，那时没有 localStorage，`persist` 会以初始值（空组织 + 默认视图）渲染，
// 客户端挂载后再 rehydrate。这里**不会**产生 hydration mismatch：组织只影响已配对设备的渲染，
// 而设备来自运行时节点——构建期一台都没有，预渲染出来的必然是空态；视图偏好同理，
// 没有文件时文件浏览器整块不渲染。将来若有别的东西也读这份偏好，且它在预渲染时就有内容，
// 就要重新考虑这一条。

import { useStore } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import { createStore } from "zustand/vanilla";
import {
  DEFAULT_FILE_BROWSER_VIEWS,
  emptyDeviceOrganization,
  normalizeDeviceOrganization,
  normalizeFileBrowserViews,
  sortDeviceGroups,
  type DeviceOrganization,
  type FileBrowserScope,
  type FileBrowserView,
} from "@swarmdrop/shared-view";

export interface PreferencesState {
  /** 本机对已配对设备的别名与分组。不同步到对端。 */
  deviceOrganization: DeviceOrganization;
  /**
   * 文件浏览器的视图偏好，**按场景分别记忆**。默认值与归一规则都在共享包里
   * （三端同一份，见 `@swarmdrop/shared-view` 的 `view-preference.ts`）。
   */
  fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
}

const STORAGE_KEY = "swarmdrop:preferences";

export const preferencesStore = createStore<PreferencesState>()(
  persist(
    () => ({
      deviceOrganization: emptyDeviceOrganization,
      fileBrowserViews: { ...DEFAULT_FILE_BROWSER_VIEWS },
    }),
    {
      name: STORAGE_KEY,
      storage: createJSONStorage(() => localStorage),
      // 磁盘上的值可能是旧版本写的、缺字段、或被手改坏。组织的归一放在共享包里，三端同一份
      // 降级规则（丢弃非法分组、剔除悬空成员关系与空别名）。
      merge: (persisted, current) => {
        const saved = persisted as Partial<PreferencesState> | undefined;
        return {
          ...current,
          deviceOrganization: normalizeDeviceOrganization(saved?.deviceOrganization),
          fileBrowserViews: normalizeFileBrowserViews(saved?.fileBrowserViews),
        };
      },
    },
  ),
);

/**
 * React 侧订阅入口。**selector 只许返回原始值或 store 内的稳定引用**——与 `store.ts` 同一条
 * 约束，`pnpm check:zustand-access` 的规则 B 覆盖本目录。派生放组件体内的 `useMemo`。
 */
export function usePreferences<U>(selector: (state: PreferencesState) => U): U {
  return useStore(preferencesStore, selector);
}

/** 偏好的写入口。全部走这里，组件不直接 `setState`。 */
export const preferencesActions = {
  /** 记住某个场景下用户选的视图。 */
  setFileBrowserView(scope: FileBrowserScope, view: FileBrowserView) {
    preferencesStore.setState((s) =>
      s.fileBrowserViews[scope] === view
        ? // 「内容没变」返回 state 本身（同下面 forgetDevice 的理由）——视图切换按钮的
          // 每次点击都会调到这里，其中「点当前视图」那一半本该是无操作。
          s
        : { fileBrowserViews: { ...s.fileBrowserViews, [scope]: view } },
    );
  },

  /** 设别名；传空串/纯空白即清除。 */
  setDeviceAlias(peerId: string, alias: string) {
    preferencesStore.setState((s) => {
      const trimmed = alias.trim();
      const aliases = { ...s.deviceOrganization.aliases };
      if (trimmed) aliases[peerId] = trimmed;
      else delete aliases[peerId];
      return { deviceOrganization: { ...s.deviceOrganization, aliases } };
    });
  },

  /** 新建分组，返回它的 id；名称为空则不建并返回 null。 */
  createGroup(name: string): string | null {
    const groupName = name.trim();
    if (!groupName) return null;
    const id = crypto.randomUUID();
    preferencesStore.setState((s) => ({
      deviceOrganization: {
        ...s.deviceOrganization,
        groups: [
          ...s.deviceOrganization.groups,
          { id, name: groupName, sortOrder: s.deviceOrganization.groups.length },
        ],
      },
    }));
    return id;
  },

  renameGroup(groupId: string, name: string) {
    const groupName = name.trim();
    if (!groupName) return;
    preferencesStore.setState((s) => ({
      deviceOrganization: {
        ...s.deviceOrganization,
        groups: s.deviceOrganization.groups.map((group) =>
          group.id === groupId ? { ...group, name: groupName } : group,
        ),
      },
    }));
  },

  deleteGroup(groupId: string) {
    preferencesStore.setState((s) => {
      const groupDeviceIds = { ...s.deviceOrganization.groupDeviceIds };
      delete groupDeviceIds[groupId];
      return {
        deviceOrganization: {
          ...s.deviceOrganization,
          // `groups` 数组保持插入序，`sortOrder` 才是用户自定义顺序的载体——删组后必须
          // **先按 sortOrder 排序再重新编号**，否则会把用户排过的顺序退回插入序。
          groups: sortDeviceGroups(
            s.deviceOrganization.groups.filter((group) => group.id !== groupId),
          ).map((group, sortOrder) => ({ ...group, sortOrder })),
          groupDeviceIds,
        },
      };
    });
  },

  /** 设定某台设备所属的分组集合（全量覆盖，不是增量）。 */
  setDeviceGroups(peerId: string, groupIds: string[]) {
    preferencesStore.setState((s) => {
      const valid = new Set(s.deviceOrganization.groups.map((group) => group.id));
      const selected = new Set(groupIds.filter((id) => valid.has(id)));
      const groupDeviceIds: Record<string, string[]> = {};
      for (const group of s.deviceOrganization.groups) {
        const members = new Set(s.deviceOrganization.groupDeviceIds[group.id] ?? []);
        if (selected.has(group.id)) members.add(peerId);
        else members.delete(peerId);
        groupDeviceIds[group.id] = [...members];
      }
      return { deviceOrganization: { ...s.deviceOrganization, groupDeviceIds } };
    });
  },

  /** 解除配对后清掉该设备的别名与分组成员关系，别让它以幽灵形式留在偏好里。 */
  forgetDevice(peerId: string) {
    preferencesStore.setState((s) => {
      const hasAlias = peerId in s.deviceOrganization.aliases;
      const memberOf = Object.entries(s.deviceOrganization.groupDeviceIds).filter(
        ([, ids]) => ids.includes(peerId),
      );
      // 「内容没变」返回 state 本身而不是 `{}`——zustand 判的是 `Object.is(partial, state)`，
      // 空对象是新对象、判不等，会照常广播一轮（store.ts 里记的是同一条）。
      if (!hasAlias && memberOf.length === 0) return s;

      const aliases = { ...s.deviceOrganization.aliases };
      delete aliases[peerId];
      const groupDeviceIds = { ...s.deviceOrganization.groupDeviceIds };
      for (const [groupId, ids] of memberOf) {
        groupDeviceIds[groupId] = ids.filter((id) => id !== peerId);
      }
      return { deviceOrganization: { aliases, groups: s.deviceOrganization.groups, groupDeviceIds } };
    });
  },
};
