// 事件源二：pairing 入站请求轮询。
//
// pairing 请求**不在 events() 流里**——内核侧走 NetManager 的 WebEventBus，`WebNode` 只暴露
// `pending_pairing_requests()`（取出即清空）。所以唯一入口是定时轮询。这是与桌面 Tauri 事件
// 推送不同的地方（浏览器侧未把 pairing 做成推送流），基座如实承载这条轮询轨。

import { webNodeActions } from "./store";
import type { WebNode } from "./view-types";

const POLL_INTERVAL_MS = 1500;

/** 开始轮询，返回停止函数（clearInterval）。 */
export function startPairingPoll(node: WebNode): () => void {
  const timer = setInterval(() => {
    try {
      const reqs = node.pending_pairing_requests();
      webNodeActions.addPendingPairings(reqs);
    } catch {
      // 节点关停后调用会抛；忽略即可（调用方通常紧接着 clearInterval）。
    }
  }, POLL_INTERVAL_MS);
  return () => clearInterval(timer);
}
