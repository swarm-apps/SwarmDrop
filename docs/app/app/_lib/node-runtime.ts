// WebNode 运行时：动态加载 wasm 模块 + spawn/close 单例。
//
// 关键约束（Next `output: "export"` 静态导出 + `reactStrictMode: true`）：
//   - 运行时禁止顶层静态 import wasm——预渲染在 Node 里加载会挂。一律动态 import。
//   - StrictMode 下 effect 会 double-invoke。这里用 module 级记忆化保证「一个页面一个节点」：
//     重复调用 spawnNode() 复用同一 Promise / 同一实例，绝不 spawn 出两个（events 只能取一次，
//     两个节点会各起一条流互相抢）。

import type { SwarmdropWebModule, WebNode } from "./view-types";

let modulePromise: Promise<SwarmdropWebModule> | null = null;
let node: WebNode | null = null;
let spawnPromise: Promise<WebNode> | null = null;

async function loadModule(): Promise<SwarmdropWebModule> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const mod = (await import("swarmdrop-web")) as unknown as SwarmdropWebModule;
      // wasm-pack --target web 的 default 导出即 init（拉 .wasm）；spawn 前必须 await 一次。
      await mod.default();
      return mod;
    })();
  }
  return modulePromise;
}

/** 拿到（或建立）页面级唯一 WebNode。并发/重复调用复用同一 Promise。 */
export function spawnNode(): Promise<WebNode> {
  if (node) return Promise.resolve(node);
  if (!spawnPromise) {
    spawnPromise = loadModule()
      .then((mod) => mod.WebNode.spawn())
      .then((n) => {
        node = n;
        return n;
      })
      .catch((e) => {
        // 失败清空以允许重试（下次 spawnNode 重新走一遍）。
        spawnPromise = null;
        throw e;
      });
  }
  return spawnPromise;
}

export function getNode(): WebNode | null {
  return node;
}

/** 显式关停（NetManager::shutdown + 关 Endpoint）。供设置/退出流程调用。 */
export async function closeNode(): Promise<void> {
  const current = node;
  node = null;
  spawnPromise = null;
  if (current) await current.close();
}
