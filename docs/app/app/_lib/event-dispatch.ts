// 事件源一：transfer 域事件流的单点消费。
//
// `events()` 返回的 ReadableStream **只能取一次**（内核侧 take rx）。这里用 module 级守卫保证
// 全进程只 getReader 一次——StrictMode 双 mount、多组件挂载都不会重复取流（重复取会拿到已被
// 消费的 rx / 抛错）。读到的每条事件派发进 store 的 reducer。

import { webNodeActions } from "./store";
import type { WebNode, WebTransferEvent } from "./view-types";

let consuming = false;

/** 启动单点消费；已在消费则直接返回（幂等）。 */
export function startEventConsumption(node: WebNode): void {
  if (consuming) return;
  consuming = true;
  void consume(node);
}

async function consume(node: WebNode): Promise<void> {
  let reader: ReadableStreamDefaultReader<WebTransferEvent>;
  try {
    reader = node.events().getReader();
  } catch (e) {
    // events() 已被取走或节点异常：不再重试，交由上层日志。
    consuming = false;
    console.error("[web] events() 获取失败", e);
    return;
  }

  for (;;) {
    let done: boolean;
    let value: WebTransferEvent | undefined;
    try {
      ({ done, value } = await reader.read());
    } catch (e) {
      console.error("[web] 事件流读取中断", e);
      break;
    }
    if (done) break;
    // 序列化失败时 sink 侧产 NULL（已在 Rust 端 warn），跳过。
    if (!value) continue;
    try {
      webNodeActions.applyEvent(value);
    } catch (e) {
      console.error(`[web] 处理事件 ${value.type} 抛错（已跳过）`, e);
    }
  }
  consuming = false;
}
