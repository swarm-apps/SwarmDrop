// SwarmDrop Web Worker 入口（module worker）：wasm 节点整个跑在 Worker 里，
// 主线程经 postMessage RPC 驱动（配 client.js）。
//
// 约束：Worker 里只拨 ws / circuit 地址——webrtc-websys 的 dial 碰 window 会 panic
// （构造无害，见 crates/web/src/env.rs 模块 doc）。身份存 OPFS（Worker 无 localStorage）。
import init, { WebNode } from "./pkg/swarmdrop_web.js";

let node = null;

// 事件流单点消费 → 逐条转发主线程（事件是 plain object，structured clone 直接过）
async function pumpEvents() {
  const reader = node.events().getReader();
  while (true) {
    let r;
    try {
      r = await reader.read();
    } catch (e) {
      self.postMessage({ type: "eventsError", error: String(e) });
      return;
    }
    if (r.done) return;
    if (r.value) self.postMessage({ type: "event", event: r.value });
  }
}

self.onmessage = async (m) => {
  const { id, method, args = [] } = m.data;
  try {
    let value;
    if (method === "spawn") {
      await init({ module_or_path: "./pkg/swarmdrop_web_bg.wasm?v=" + Date.now() });
      node = await WebNode.spawn();
      pumpEvents();
      value = node.node_id();
    } else if (!node) {
      throw new Error("未 spawn");
    } else {
      value = await node[method](...args);
    }
    self.postMessage({ id, ok: true, value });
  } catch (e) {
    self.postMessage({ id, ok: false, error: e?.message ?? String(e) });
  }
};
