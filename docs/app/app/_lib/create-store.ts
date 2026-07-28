// 零依赖的极简外部 store（订阅 + 快照 + 浅合并 setState），配 React `useSyncExternalStore`。
//
// 为什么不引 zustand：docs 的 pnpm 全局 store 正从 v10 迁往 v11，`node_modules` 还链在旧
// store 上，`pnpm add` 会触发全量 reinstall + 改 lockfile。为装一个状态库污染迁移中的环境不
// 划算。这里的 store 模式（subscribe / getSnapshot / actions）与桌面 `network-store` 的
// zustand 心智一致，后续要换回 zustand 时调用面几乎不变，可平滑替换。

import { useSyncExternalStore } from "react";

export interface Store<T> {
  getState: () => T;
  /** SSR / 静态导出预渲染用的初始快照（`output: "export"` 会在 Node 里跑一次）。 */
  getInitialState: () => T;
  setState: (partial: Partial<T> | ((state: T) => Partial<T>)) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createStore<T extends object>(initial: T): Store<T> {
  let state = initial;
  const listeners = new Set<() => void>();

  const getState = () => state;
  const getInitialState = () => initial;

  const setState: Store<T>["setState"] = (partial) => {
    const next =
      typeof partial === "function"
        ? (partial as (state: T) => Partial<T>)(state)
        : partial;
    // 浅比较：所有键都未变则不通知，避免无谓 re-render。
    let changed = false;
    for (const key in next) {
      const k = key as keyof T;
      if (!Object.is(next[k], state[k])) {
        changed = true;
        break;
      }
    }
    if (!changed) return;
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  };

  const subscribe: Store<T>["subscribe"] = (listener) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  };

  return { getState, getInitialState, setState, subscribe };
}

// selector 必须返回原始值或 store 内稳定引用；不要在 selector 里 map/filter/slice 造新引用，
// 否则 useSyncExternalStore 每次快照不等，触发无限 re-render（参见桌面 zustand 派生数组陷阱）。
export function useStore<T extends object, U>(
  store: Store<T>,
  selector: (state: T) => U,
): U {
  return useSyncExternalStore(
    store.subscribe,
    () => selector(store.getState()),
    () => selector(store.getInitialState()),
  );
}
