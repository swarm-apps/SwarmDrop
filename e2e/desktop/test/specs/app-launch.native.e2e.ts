import { expect, browser, $ } from "@wdio/globals";

describe("SwarmDrop 桌面壳（native 模式）", () => {
  it("应能启动并加载出 React 根节点", async () => {
    // 冷启动要跑 P2P 网络栈 + SQLite 初始化，标题/DOM 不是瞬间就绪，用带轮询的断言而不是立即断言。
    await expect(browser).toHaveTitle("SwarmDrop", { wait: 30_000 });
    await expect($("#root")).toBeExisting();
  });
});
