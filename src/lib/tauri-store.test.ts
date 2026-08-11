/**
 * `flushTauriStores` 与写入串行化的护栏测试。
 *
 * 「退出前先 flush」那条顺序不变量由 `quit-app.test.ts` 看守，这里只管 flush 自身：
 * 待决写入必须等到、卡死时必须有上限、同一文件的写入必须按调用序到达后端
 * （persist 写的是全量快照，乱序 = 后一次被前一次的旧快照覆盖）。
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

import { FLUSH_DEADLINE_MS } from "./tauri-store";

/** 受控假 Store：每次 `set` 都挂起到测试放行，把「写入还在路上」这个瞬间固定住。 */
const fake = vi.hoisted(() => {
  const order: string[] = [];
  const gates: Array<() => void> = [];
  return {
    order,
    /** 当前挂起的 set 数量——串行化生效时恒 ≤ 1。 */
    inFlight: () => gates.length,
    releaseNext: () => gates.shift()?.(),
    reset: () => {
      order.length = 0;
      gates.length = 0;
    },
    store: {
      get: async () => null,
      set: async (_name: string, value: string) => {
        await new Promise<void>((resolve) => gates.push(resolve));
        order.push(`set:${value}`);
      },
      delete: async () => {
        await new Promise<void>((resolve) => gates.push(resolve));
        order.push("delete");
      },
      save: async () => {
        order.push("save");
      },
    },
  };
});

vi.mock("@tauri-apps/plugin-store", () => ({
  load: async () => fake.store,
}));

/** 让挂起的 promise 链推进到下一个 await。 */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("flushTauriStores", () => {
  beforeEach(() => {
    // 模块级的 stores / pendingWrites / writeChains 是单例，逐用例重置避免串扰
    vi.resetModules();
    fake.reset();
  });

  it("等待未完成的写入到达后端，再显式落盘", async () => {
    const { createTauriStorage, flushTauriStores } = await import("./tauri-store");
    const storage = createTauriStorage("preferences.json");

    // zustand persist 不 await setItem，这里保持同样的 fire-and-forget 形态
    void storage.setItem("preferences-store", "quit");

    let flushed = false;
    const flush = flushTauriStores().then(() => {
      flushed = true;
    });

    await settle();
    expect(flushed).toBe(false);
    expect(fake.order).toEqual([]);

    fake.releaseNext();
    await flush;
    expect(fake.order).toEqual(["set:quit", "save"]);
  });

  it("删除同样计入待决写入", async () => {
    const { createTauriStorage, flushTauriStores } = await import("./tauri-store");
    const storage = createTauriStorage("preferences.json");

    void storage.removeItem("preferences-store");

    let flushed = false;
    const flush = flushTauriStores().then(() => {
      flushed = true;
    });

    await settle();
    expect(flushed).toBe(false);

    fake.releaseNext();
    await flush;
    expect(fake.order).toEqual(["delete", "save"]);
  });

  it("同一文件的写入串行到达后端，不会乱序覆盖", async () => {
    const { createTauriStorage, flushTauriStores } = await import("./tauri-store");
    const storage = createTauriStorage("preferences.json");

    void storage.setItem("preferences-store", "A");
    void storage.setItem("preferences-store", "B");

    await settle();
    expect(fake.inFlight()).toBe(1); // A 在飞，B 还没发出
    fake.releaseNext();

    await settle();
    expect(fake.inFlight()).toBe(1); // A 落地后 B 才发出，两者从不同时在飞
    fake.releaseNext();

    await flushTauriStores();
    expect(fake.order).toEqual(["set:A", "set:B", "save"]);
  });

  it("写入卡死时仍在上限内返回，退出不被拖住", async () => {
    vi.useFakeTimers();
    try {
      const { createTauriStorage, flushTauriStores } = await import("./tauri-store");
      const storage = createTauriStorage("preferences.json");

      void storage.setItem("preferences-store", "never"); // 永不放行
      const flush = flushTauriStores();

      await vi.advanceTimersByTimeAsync(FLUSH_DEADLINE_MS);
      await expect(flush).resolves.toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });
});
