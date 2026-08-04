// 事件源四：relay 意图的状态流。
//
// ## 为什么在运行时层而不是「连接」面板里
//
// relay 状态是**节点的属性**，不是设置页的。它有两个消费者，分处两条路由：
//
//   设置页「连接」区   —— 列出每条 relay 的状态、失败原因，提供移除
//   设备页「配对」区   —— 读 circuit 可达地址，决定能不能生成邀请
//
// 订阅若挂在「连接」面板里，配对面板就只有在用户**进过一次设置页**之后才知道自己可达：
// 直接进设备页的用户会看到「生成邀请」被禁用 + 一句「去设置页连接中继」，而中继其实早在
// 启动时就自动连上了（`ensureConfiguredRelays`）。这正是「运行时单例只挂 layout」那条规矩
// 要防的东西——把跨路由的事实绑在某一个页面上。
//
// ## 与 `events()` 的关键差别：可以多次订阅
//
// `relays_changed()` 每次调用返回一条**独立**的流（`node.rs` 的文档写明了），不像 `events()`
// 那样取走就没了。所以这里不需要 `event-dispatch.ts` 那种模块级单点守卫；但仍然只在 layout
// 起一次，理由是上面那条——多起几份只是浪费，不是错误。

import { webNodeActions } from "./store";
import type { RelayInfoJson, WebNode } from "./view-types";

/**
 * 订阅 relay 状态流，返回停止函数。
 *
 * 首帧先读一次 `relays_state()` 快照：流只在**变化时**产出，不补首帧。少了它，页面在
 * 下一次 relay 状态变动之前会一直显示空列表。
 */
export function startRelayWatch(node: WebNode): () => void {
  try {
    webNodeActions.setRelays(node.relays_state() as RelayInfoJson[]);
  } catch (e) {
    console.error("[web] relays_state() 失败", e);
  }

  const reader = (node.relays_changed() as ReadableStream<RelayInfoJson[]>).getReader();
  let stopped = false;

  void (async () => {
    try {
      for (;;) {
        const { done, value } = await reader.read();
        // `stopped` 与 `done` 都要判：`cancel()` 会让在途的 `read()` 以 `done: true` 兑现，
        // 但先于它兑现的那一次仍可能带着值回来。
        if (done || stopped) return;
        if (value) webNodeActions.setRelays(value);
      }
    } catch (e) {
      if (!stopped) console.error("[web] relays_changed() 中断", e);
    }
  })();

  return () => {
    stopped = true;
    void reader.cancel().catch(() => {});
  };
}
