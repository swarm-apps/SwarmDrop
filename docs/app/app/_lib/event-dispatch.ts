// 事件源一：transfer 域事件流的单点消费。
//
// ## 守卫按「节点实例」换代，不是一个全局布尔
//
// `events()` 返回的 ReadableStream **每个节点实例只能取一次**（内核侧 take rx），所以这里
// 必须有守卫。但它记的是**当前正在消费哪个实例**而不是「有没有人在消费」：
//
//   同一实例重复调用   直接返回（StrictMode 双 mount、多组件挂载都不会重复取流）
//   换了一个新实例     先停旧的，再取新的
//
// 后一条是节点可停可起之后才出现的：用户在弹窗里停掉节点再启动，`spawnNode()` 会给出一个
// **新的** WebNode。若守卫只是个布尔，旧流的 `done` 还没落地时新实例就撞上「已经在消费了」
// 而被静默跳过——节点看起来跑着，传输事件却一条都不到，进度永远停在 0。

import { webNodeActions } from "./store";
import type { WebNode, WebTransferEvent } from "./view-types";

type Consumption = {
  node: WebNode;
  reader: ReadableStreamDefaultReader<WebTransferEvent>;
};

let current: Consumption | null = null;

/** 开始消费该实例的事件流。同一实例幂等；换实例则接管。 */
export function startEventConsumption(node: WebNode): void {
  if (current?.node === node) return;
  stopEventConsumption();

  let reader: ReadableStreamDefaultReader<WebTransferEvent>;
  try {
    reader = node.events().getReader();
  } catch (e) {
    // events() 已被取走或节点异常：不再重试，交由上层日志。
    console.error("[web] events() 获取失败", e);
    return;
  }

  const consumption: Consumption = { node, reader };
  current = consumption;
  void consume(consumption);
}

/** 停止消费（关停节点时调用）。取消 reader 会让在途的 `read()` 以 `done: true` 兑现。 */
export function stopEventConsumption(): void {
  const consumption = current;
  if (!consumption) return;
  current = null;
  void consumption.reader.cancel().catch(() => {});
}

async function consume(consumption: Consumption): Promise<void> {
  const { reader } = consumption;
  for (;;) {
    let done: boolean;
    let value: WebTransferEvent | undefined;
    try {
      ({ done, value } = await reader.read());
    } catch (e) {
      // 主动取消走的是 `done: true` 那条路；这里只可能是真的中断。
      if (current === consumption) console.error("[web] 事件流读取中断", e);
      break;
    }
    if (done) break;
    // 序列化失败时 sink 侧产 NULL（已在 Rust 端 warn），跳过。
    if (!value) continue;
    // 换代之后旧流可能还有在途的一帧。丢掉它——那是上一个节点的事，落进 store 会污染新节点。
    if (current !== consumption) break;
    try {
      webNodeActions.applyEvent(value);
    } catch (e) {
      console.error(`[web] 处理事件 ${value.type} 抛错（已跳过）`, e);
    }
  }
  if (current === consumption) current = null;
}
