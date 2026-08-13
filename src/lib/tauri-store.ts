/**
 * Tauri Plugin Store 的 Zustand StateStorage 适配器
 * 将 tauri-plugin-store 封装为 Zustand persist 中间件可用的 StateStorage
 */

import { load, type Store } from "@tauri-apps/plugin-store";
import type { StateStorage } from "zustand/middleware";

/** 已加载的 Store 实例缓存 */
const stores = new Map<string, Store>();

/**
 * 已发起、尚未到达后端的写入。
 *
 * zustand persist 在 `set()` 里**同步**调用 `storage.setItem()` 但不 await 它，
 * 所以「偏好已改」与「后端已收到」之间隔着一次 IPC 往返。{@link flushTauriStores}
 * 等的就是这些。
 */
const pendingWrites = new Set<Promise<unknown>>();

/**
 * 每个 store 文件的写入串行链。
 *
 * persist 每次写的是**全量快照**，所以两次写入乱序到达 = 后一次被前一次的旧快照覆盖。
 * 并发路径是真实存在的：首次 `setItem` 要等 `load()`，紧随其后走缓存的第二次会先到。
 */
const writeChains = new Map<string, Promise<unknown>>();

/** {@link flushTauriStores} 的兜底上限，超时即放弃等待、绝不让退出卡住。 */
export const FLUSH_DEADLINE_MS = 2_000;

/** 发起一次写入：接在本文件的串行链尾部，并登记进 {@link pendingWrites}。 */
function write(path: string, op: (store: Store) => Promise<unknown>): Promise<void> {
  const previous = writeChains.get(path) ?? Promise.resolve();
  const pending = (async () => {
    await previous; // 链上的每一环都自己吞掉失败，故这里不会 reject
    try {
      await op(await getStore(path));
    } catch (err) {
      // 唯一的失败信号：调用方（persist）丢弃了这个 promise，不报就彻底无声无息
      console.error(`tauri store write failed (${path}):`, err);
    }
  })();
  writeChains.set(path, pending);
  pendingWrites.add(pending);
  void pending.finally(() => pendingWrites.delete(pending));
  return pending;
}

/** 等所有待决写入到达后端，再显式落盘。 */
async function drainAndSave(): Promise<void> {
  // 等待期间 persist 可能又写一次（如 hydration 后的副作用），所以要等到队列空为止
  while (pendingWrites.size > 0) {
    await Promise.allSettled([...pendingWrites]);
  }
  await Promise.allSettled([...stores.values()].map((store) => store.save()));
}

/**
 * 等待经本模块发起的写入到达后端并落盘。终止进程前调用，见 `@/lib/quit-app`。
 *
 * 不可替代的是「到达后端」这半件事：`store.set()` 是一次 IPC，而 persist 不 await 它。
 * 值一旦进了后端内存就安全了——插件自己会在 `RunEvent::Exit` 时保存所有 store
 * （tauri-plugin-store `lib.rs`，且 `Store::save` 会取消 `autoSave` 的 100ms 防抖）。
 * 这里的 `save()` 因此是不依赖那条兜底的双保险，不是修复本身，别把它当关键路径而删掉 drain。
 *
 * 作用域仅限 {@link createTauriStorage} 建的 store（今天只有 zustand persist 在用）。
 * `tauri-adapter.ts` 的更新器 store 是另一条通路，它每次 set 后自己 `save()`，不在此列。
 */
export function flushTauriStores(): Promise<void> {
  // 超时后进程紧接着就没了，不必 clearTimeout 这个悬空定时器
  return Promise.race([
    drainAndSave(),
    new Promise<void>((resolve) => setTimeout(resolve, FLUSH_DEADLINE_MS)),
  ]);
}

/**
 * 延迟加载指定的 Tauri Store
 * 在首次访问时才初始化，避免模块顶层副作用
 */
async function getStore(path: string): Promise<Store> {
  let store = stores.get(path);
  if (!store) {
    store = await load(path, { defaults: {}, autoSave: true });
    stores.set(path, store);
  }
  return store;
}

/**
 * 创建基于 tauri-plugin-store 的 Zustand StateStorage
 *
 * @param path 存储文件名，保存在应用配置目录下（如 "preferences.json"）
 *
 * @example
 * ```ts
 * import { createJSONStorage, persist } from "zustand/middleware";
 * import { createTauriStorage } from "@/lib/tauri-store";
 *
 * persist(storeCreator, {
 *   name: "my-store",
 *   storage: createJSONStorage(() => createTauriStorage("my-store.json")),
 * })
 * ```
 */
export function createTauriStorage(path: string): StateStorage {
  return {
    getItem: async (name: string) => {
      const store = await getStore(path);
      return (await store.get<string>(name)) ?? null;
    },
    setItem: (name: string, value: string) =>
      write(path, (store) => store.set(name, value)),
    removeItem: (name: string) => write(path, (store) => store.delete(name)),
  };
}
