// 轮询四类同步 getter：
//   源一：pairing 入站请求——内核侧走 NetManager 的 WebEventBus，`pending_pairing_requests()`
//         取出即清空。这是与桌面 Tauri 事件推送不同的地方（浏览器侧未把 pairing 做成推送流）。
//   源二（#77）：已配对设备清单——`paired_devices()` 是幂等快照读（presence 在线状态会变，
//         需要定时刷新才不显示陈旧状态）。内核其实已有 `DevicesChanged`/`PairedDeviceAdded`
//         事件（`WebEventBus::publish` 目前把它们吞进日志，未 surface），轮询是当前的权宜之
//         计——见 crates/web/README.md「遗留/取舍」。
//   源三（#79）：挂起入站 offer——`pending_offers()` 也是幂等快照读，用于补回事件流接管前
//         已经到达的请求，避免刷新/启动时「后端仍待确认、前端却不显示」。
//   源四：已连接的 SwarmDrop 客户端数——`connected_peers()` 与桌面/移动的
//         `NetworkStatus.connected_peers` **是同一个函数**（core 的 `DeviceManager::
//         connected_count()`，按 `is_swarmdrop_agent` 过滤，bootstrap/relay 不算）。
//         Web 此前没有这个绑定，设置页只能拿「已配对设备里在线的台数」凑数，而那是
//         presence 快照，未配对的对端不在里面。
// 四者调用成本都很低（本地内存读），合并成一个 timer 而非各开一个；但各自独立 try/catch，
// 避免一个抛错连带跳过另一个的刷新。

import { webNodeActions } from "./store";
import type { WebNode } from "./view-types";

const POLL_INTERVAL_MS = 1500;

/** 开始轮询，返回停止函数（clearInterval）。 */
export function startStatePoll(node: WebNode): () => void {
  const tick = () => {
    try {
      webNodeActions.addPendingPairings(node.pending_pairing_requests());
    } catch {
      // 节点关停后调用会抛；忽略即可（调用方通常紧接着 clearInterval）。
    }
    try {
      webNodeActions.setPairedDevices(node.paired_devices());
    } catch {
      // ignore，理由同上。
    }
    try {
      webNodeActions.setPendingOffers(node.pending_offers());
    } catch {
      // ignore，理由同上。
    }
    try {
      webNodeActions.setConnectedPeers(node.connected_peers());
    } catch {
      // ignore，理由同上。
    }
  };
  tick(); // 立即拉一次，不必等第一个 tick 才看到已配对设备。
  const timer = setInterval(tick, POLL_INTERVAL_MS);
  return () => clearInterval(timer);
}
