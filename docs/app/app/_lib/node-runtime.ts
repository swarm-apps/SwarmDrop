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

/**
 * 拿到（或加载并 init）wasm 模块本身，用于调用**模块级导出**（如设备名的三个自由函数）。
 *
 * 与 `spawnNode()` 共用同一个记忆化的 `modulePromise`，但两者的失败是独立的：
 * `spawnNode()` 出错时清的是 `spawnPromise`，`modulePromise` 不受影响。所以
 * 「节点起不来，但模块可用」是一个正常且被依赖的状态 —— 改设备名不该被节点状态绑架
 * （设置页在 `status: "error"` 时仍然可达）。
 */
export function getModule(): Promise<SwarmdropWebModule> {
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
    spawnPromise = getModule()
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

/**
 * 改设备名，返回内核归一化后的名字（`null` = 已清空，对外回落到浏览器默认名）。
 *
 * **这一行分支只能在 JS 做**：节点句柄只活在本模块里，Rust 那边够不到它，所以
 * 「有没有节点」这个判断挪不进内核（桌面与移动的同一分支在 core，它们的宿主本来就持有
 * `Option<NetManager>`）。
 *
 * - 节点在跑 → `WebNode::rename_device`：落盘 + 本机 `OsInfo` + identify 逐连接下发，
 *   已连接的对端一个 RTT 内看到新名字，不必刷新页面。
 * - 节点没起来（spawn 失败或尚未 spawn）→ 退回模块级 `set_device_name`，只落盘；
 *   新名字在节点下次启动时由组合根从 `DeviceConfig` 端口读回。
 */
export async function renameDevice(name: string | null): Promise<string | null> {
  const current = getNode();
  if (current) return (await current.rename_device(name)) ?? null;
  const mod = await getModule();
  await mod.set_device_name(name);
  // 模块级导出只落盘、不回传归一化结果，回读一次拿真正落库的那个值。
  return (await mod.get_device_name()) ?? null;
}

/** 显式关停（NetManager::shutdown + 关 Endpoint）。供设置/退出流程调用。 */
export async function closeNode(): Promise<void> {
  const current = node;
  node = null;
  spawnPromise = null;
  if (current) await current.close();
}
