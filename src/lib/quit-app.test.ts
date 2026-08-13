/**
 * `quitApp` / `relaunchApp` 的护栏测试。
 *
 * 它看守的是进程终止路径上唯一的一条不变量：**偏好写入到达后端之前，进程不许终止**。
 * 破坏它不会报错、也不会有任何日志——只会表现为「勾了『记住我的选择』再退出，
 * 下次启动照旧弹询问框」，而选「最小化到托盘」的记忆却正常（那条不杀进程）。
 *
 * 「不许绕过本模块直接 invoke」由 `scripts/check-quit-entry.mjs` 看守，不在这里。
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

const calls = vi.hoisted(() => [] as string[]);

vi.mock("@/lib/tauri-store", () => ({
  flushTauriStores: vi.fn(async () => {
    // 落盘不是瞬时的：跨一个 tick，好让「没等就终止」暴露成顺序颠倒
    await new Promise((resolve) => setTimeout(resolve, 0));
    calls.push("flush");
  }),
}));

vi.mock("@/lib/bindings", () => ({
  commands: { quitApp: vi.fn(async () => calls.push("quit")) },
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(async () => calls.push("relaunch")),
}));

describe("进程终止入口", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("quitApp 先等偏好落盘，再杀进程", async () => {
    const { quitApp } = await import("./quit-app");

    await quitApp();

    expect(calls).toEqual(["flush", "quit"]);
  });

  it("relaunchApp 同样先等偏好落盘", async () => {
    const { relaunchApp } = await import("./quit-app");

    await relaunchApp();

    expect(calls).toEqual(["flush", "relaunch"]);
  });
});
