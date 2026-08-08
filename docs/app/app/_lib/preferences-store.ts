// Web 应用区的**持久化偏好**。
//
// 与 `store.ts` 刻意分开：那份是运行时节点状态（节点一关就该没了），这份是用户的本机设置
// （关标签页也要留着）。桌面端同样是这个分工——`network-store` 运行时、`preferences-store`
// 持久化——混在一起会让「什么该在刷新后还在」变成一道要逐字段判断的题。
//
// 目前有三样：设备组织（别名 + 分组）、文件浏览器的视图偏好、以及用户对引导节点清单的
// 修改。三者都**不同步给对端**，纯本机设置，所以放 localStorage 就够，不必进 IndexedDB
// （那份是运行时状态，两者的生命周期不同）。
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
import { WEB_RELAY_HELPERS, WEB_RELAY_PEER_IDS, bootstrapPeerId } from "./relay-helpers";

/**
 * 用户对引导节点清单做过的修改。
 *
 * **存 custom 与 removed 两个集合，不存合并后的最终清单。** 后者看着更省事，但它会在新版本
 * 更换内置节点地址时把老用户**永久压在旧地址上**——他们的快照里躺着上一版的那条，而新版的
 * 那条永远不会被写进来。故障形态是「升级后突然连不上」，且用户完全无法自查（界面上那条
 * 地址看起来一点问题都没有）。分成两个集合则「内置清单」始终是代码里那份活的事实源，
 * 偏好只表达用户对它的**增删意图**。
 */
export interface InfraNodePreferences {
  /** 用户自己加的节点，存完整 multiaddr（回放要用它登记）。 */
  custom: string[];
  /**
   * 用户撤销掉的**内置**节点，存 peer id 而不是地址。
   *
   * 撤销的对象是「那个节点」，不是「那串地址」：内置项换地址（同一台机器换端口 / 加一条
   * certhash）时用户的撤销仍应生效；换成另一台机器（peer id 变了）时则不该生效，
   * 他们从没对新的那台表过态。用 peer id 做键，两种情形自然都对。
   */
  removed: string[];
}

export interface PreferencesState {
  /** 本机对已配对设备的别名与分组。不同步到对端。 */
  deviceOrganization: DeviceOrganization;
  /**
   * 文件浏览器的视图偏好，**按场景分别记忆**。默认值与归一规则都在共享包里
   * （三端同一份，见 `@swarmdrop/shared-view` 的 `view-preference.ts`）。
   */
  fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
  /** 引导节点清单的增删意图，节点启动时回放（见 `node-lifecycle.ts`）。 */
  infraNodes: InfraNodePreferences;
}

const emptyInfraNodes: InfraNodePreferences = { custom: [], removed: [] };

/** 磁盘上的值可能是旧版本写的 / 被手改坏，一律降级成合法形状。 */
function normalizeInfraNodes(saved: unknown): InfraNodePreferences {
  if (saved === null || typeof saved !== "object") return emptyInfraNodes;
  const raw = saved as Partial<Record<keyof InfraNodePreferences, unknown>>;
  const strings = (value: unknown): string[] =>
    Array.isArray(value)
      ? [...new Set(value.filter((v): v is string => typeof v === "string" && v.trim() !== ""))]
      : [];
  return { custom: strings(raw.custom), removed: strings(raw.removed) };
}

/**
 * 节点启动时要登记的地址清单：**内置清单 − removed + custom**。
 *
 * 每次启动现算，所以换版本换地址时老用户立刻拿到新的那条——这正是「存两个集合而不是存
 * 合并快照」买到的东西，也是这个函数唯一值得单测的性质。
 *
 * 入参只收 `InfraNodePreferences` 而不是整份 `PreferencesState`：它跟设备组织与文件浏览器
 * 偏好没有半点关系，收全份只会让调用方以为这里可能读别的。
 */
export function infraNodesToReplay(prefs: InfraNodePreferences): string[] {
  const removed = new Set(prefs.removed);
  const builtin = WEB_RELAY_HELPERS.filter((addr) => {
    const peerId = bootstrapPeerId(addr);
    return peerId === null || !removed.has(peerId);
  });
  return [...builtin, ...prefs.custom];
}

const STORAGE_KEY = "swarmdrop:preferences";

export const preferencesStore = createStore<PreferencesState>()(
  persist(
    () => ({
      deviceOrganization: emptyDeviceOrganization,
      fileBrowserViews: { ...DEFAULT_FILE_BROWSER_VIEWS },
      infraNodes: emptyInfraNodes,
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
          infraNodes: normalizeInfraNodes(saved?.infraNodes),
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

  /**
   * 记住用户加了一条引导节点。
   *
   * **加回一条曾被撤销的内置项走的也是这里**，但它只从 `removed` 里划掉、不进 `custom`
   * ——否则用户手上会多出一份内置地址的**副本**，将来内置那条换了地址，副本仍指着旧的，
   * 正是两个集合要避开的那个故障。
   */
  addInfraNode(addr: string) {
    const trimmed = addr.trim();
    if (!trimmed) return;
    preferencesStore.setState((s) => {
      const peerId = bootstrapPeerId(trimmed);
      if (peerId !== null && WEB_RELAY_PEER_IDS.has(peerId)) {
        if (!s.infraNodes.removed.includes(peerId)) return s;
        return {
          infraNodes: {
            ...s.infraNodes,
            removed: s.infraNodes.removed.filter((id) => id !== peerId),
          },
        };
      }
      if (s.infraNodes.custom.includes(trimmed)) return s;
      return { infraNodes: { ...s.infraNodes, custom: [...s.infraNodes.custom, trimmed] } };
    });
  },

  /**
   * 记住用户撤销了某个引导节点（按 peer id，那是清单与内核状态之间唯一的连接键）。
   *
   * 内置项进 `removed`，自定义项则直接从 `custom` 里划掉——后者没必要记「撤销过」，
   * 不在清单里就不会被回放。两件事同时做：万一某条自定义地址与内置项是同一台机器，
   * 两边都得清干净，否则下次启动它又回来了。
   */
  forgetInfraNode(peerId: string) {
    preferencesStore.setState((s) => {
      const custom = s.infraNodes.custom.filter((addr) => bootstrapPeerId(addr) !== peerId);
      const removed =
        WEB_RELAY_PEER_IDS.has(peerId) && !s.infraNodes.removed.includes(peerId)
          ? [...s.infraNodes.removed, peerId]
          : s.infraNodes.removed;
      // 「内容没变」返回 state 本身而不是 `{}`（同 forgetDevice 的理由）。
      if (custom.length === s.infraNodes.custom.length && removed === s.infraNodes.removed) {
        return s;
      }
      return { infraNodes: { custom, removed } };
    });
  },
};
