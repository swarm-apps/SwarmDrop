/**
 * 浏览器端 relay helper 配置。
 *
 * 浏览器只能连接 WebRTC Direct 或 WSS helper；地址通过文档站构建环境注入，避免与
 * 桌面/移动的 TCP、QUIC 节点清单耦合。多个地址用英文逗号分隔，每项必须是完整 multiaddr。
 */
const DEFAULT_WEB_RELAY_HELPERS = [
  "/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

export const WEB_RELAY_HELPERS = (process.env.NEXT_PUBLIC_SWARMDROP_WEB_RELAY_HELPERS ?? DEFAULT_WEB_RELAY_HELPERS.join(","))
  .split(",")
  .map((addr) => addr.trim())
  .filter(Boolean);

/**
 * multiaddr 尾部的 peer id。
 *
 * **这是内置清单与内核状态之间唯一的连接键**：一个节点可以有多条地址（内核会把它们合并进
 * 同一条 `InfraLink`），所以「这一条是不是内置的」只能靠 peer id 比对，不能比地址串。
 * 撤销记录（`preferences-store` 的 `removed`）同样用它当键。
 */
export function bootstrapPeerId(addr: string): string | null {
  // **按段切，不能 `slice(lastIndexOf("/p2p/") + 5)`**：那样对 circuit 地址会返回
  // `RELAY/p2p-circuit`（`/p2p-circuit` 本身不含 `/p2p/`，匹配不到，于是尾段被一起带出来）。
  // 当前只喂 `WEB_RELAY_HELPERS`（纯地址）碰不到，但这个函数是导出的、且被定位成
  // 「内置清单与内核状态之间唯一的连接键」——一个静默返回垃圾的键迟早会被误用。
  const parts = addr.split("/");
  const idx = parts.lastIndexOf("p2p");
  const id = idx === -1 ? undefined : parts[idx + 1];
  return id ? id : null;
}

/** 内置引导节点的 peer id 集合。用于把内核下发的 relay 清单切成「默认 / 自定义」两半。 */
export const WEB_RELAY_PEER_IDS = new Set(
  WEB_RELAY_HELPERS.map(bootstrapPeerId).filter((id): id is string => id !== null),
);

/**
 * 地址呈现（传输名 / 短形式）统一走 `@swarmdrop/shared-view` ——
 * 桌面与本页此前各有一份逐行同构的实现，而那条「判定顺序按特异性从高到低」的不变量
 * 没有编译期保护，却只有这一侧有测试。再导出而不是让调用点直接 import，是为了让
 * 「浏览器侧的 bootstrap 呈现从这里拿」这条模块边界保持不变。
 */
export {
  transportFromAddr as bootstrapTransport,
  truncateAddr,
} from "@swarmdrop/shared-view";
