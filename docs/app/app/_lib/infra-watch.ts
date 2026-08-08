// 事件源四：基础设施关系（引导节点 / 中继）的状态流。
//
// ## 为什么在运行时层而不是「引导节点」面板里
//
// 它是**节点的属性**，不是设置页的。有三个消费者，分处三条路由：
//
//   设置页「引导节点」区 —— 列出每条关系的状态、失败原因，提供移除
//   设备页「配对」区     —— 读 circuit 可达地址，决定能不能生成邀请
//   常驻的节点状态徽章   —— 整体健康度（`summarizeNodeHealth` 要吃这份清单）
//
// 订阅若挂在「引导节点」面板里，配对面板就只有在用户**进过一次设置页**之后才知道自己可达：
// 直接进设备页的用户会看到「生成邀请」被禁用 + 一句「去设置页连接中继」，而中继其实早在
// 启动时就自动连上了（`replayInfraNodes`）。这正是「运行时单例只挂 layout」那条规矩
// 要防的东西——把跨路由的事实绑在某一个页面上。
//
// ## 与 `events()` 的关键差别：可以多次订阅
//
// `infra_changed()` 每次调用返回一条**独立**的流（`node.rs` 的文档写明了），不像 `events()`
// 那样取走就没了。所以这里不需要 `event-dispatch.ts` 那种模块级单点守卫；但仍然只在 layout
// 起一次，理由是上面那条——多起几份只是浪费，不是错误。

import { webNodeActions } from "./store";
import type { InfraLink, WebNode } from "./view-types";

/**
 * 立刻重取一次快照。
 *
 * **意图侧的增删要显式调它**：`infra_changed()` 的触发源是内核的 relay watch，而「用户刚
 * 添加了一条」在收敛环跑起来之前（最迟 1s）不会产生任何 relay 事件——不补这一下，新加的
 * 那条要过一秒才出现在清单里，看着像点了没反应。撤销同理。
 */
export function refreshInfraLinks(node: WebNode): void {
  try {
    webNodeActions.setInfraLinks(node.infra_links() as InfraLink[]);
  } catch (e) {
    console.error("[web] infra_links() 失败", e);
  }
}

/**
 * 订阅基础设施状态流，返回停止函数。
 *
 * 首帧先读一次 `infra_links()` 快照：流只在**变化时**产出，不补首帧。少了它，页面在
 * 下一次状态变动之前会一直显示空列表。
 */
export function startInfraWatch(node: WebNode): () => void {
  refreshInfraLinks(node);

  const reader = (node.infra_changed() as ReadableStream<InfraLink[]>).getReader();
  let stopped = false;

  void (async () => {
    try {
      for (;;) {
        const { done, value } = await reader.read();
        // `stopped` 与 `done` 都要判：`cancel()` 会让在途的 `read()` 以 `done: true` 兑现，
        // 但先于它兑现的那一次仍可能带着值回来。
        if (done || stopped) return;
        if (value) webNodeActions.setInfraLinks(value);
      }
    } catch (e) {
      if (!stopped) console.error("[web] infra_changed() 中断", e);
    }
  })();

  return () => {
    stopped = true;
    void reader.cancel().catch(() => {});
  };
}
