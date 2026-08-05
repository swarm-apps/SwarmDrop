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
 * **这是内置清单与内核状态之间唯一的连接键**：`relays_state()` 下发的 `RelayInfoJson`
 * 只有 `id`（base58 NodeId），不带地址，所以「这一条是不是内置的」只能靠 peer id 比对。
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
 * 传输名。**是专有名词，永不翻译**（DESIGN.md 的 Device Card Contract：用户会搜它、
 * 贴进 issue、跟日志比对）。
 *
 * 与桌面 `-bootstrap-nodes-section.tsx` 的 `getTransportLabel` 有一处不同：那边把
 * `webrtc-direct` 也标成 "WebRTC"，而契约里 `WebRTC` 与 `WebRTC Direct` 是两种传输
 * （前者要打洞与信令，后者是直接拨公网裸 IP）。浏览器唯一能拨的公网入口恰恰是后者，
 * 混称会让人以为换个 relay 也能用打洞地址。
 *
 * 判定顺序按**特异性从高到低**：`/webrtc-direct` 必须排在 `/webrtc` 前面，
 * 否则前者永远匹配不到。
 */
export function bootstrapTransport(addr: string): string {
  if (addr.includes("/webrtc-direct")) return "WebRTC Direct";
  if (addr.includes("/webrtc")) return "WebRTC";
  if (addr.includes("/wss")) return "WSS";
  if (addr.includes("/ws")) return "WebSocket";
  if (addr.includes("/quic")) return "QUIC";
  if (addr.includes("/tcp/")) return "TCP";
  return "P2P";
}

/**
 * 地址的短形式：保留协议头与末尾 peer id。
 *
 * 与桌面 `-bootstrap-nodes-section.tsx` 的 `truncateAddr` 同形，**但修了它的一个缺陷**：
 * 那边在 `p2pIdx > 30` 时会不留任何提示地砍掉中段。本仓的内置地址正好命中，输出是
 * `/ip4/47.115.172.218/udp/4003/w/p2p/12D3Ko…1utep` —— certhash 整段消失、切口处没有 `…`，
 * 看起来像一条完整但写错的 multiaddr，而用户会照着它去 issue 里贴。
 * 这里在 prefix 被截时补上省略号。**桌面那份也该同修**（本次未动）。
 */
export function truncateAddr(addr: string): string {
  if (addr.length <= 60) return addr;
  const p2pIdx = addr.indexOf("/p2p/");
  if (p2pIdx === -1) return `${addr.slice(0, 30)}…${addr.slice(-20)}`;
  const prefix = p2pIdx > 30 ? `${addr.slice(0, 30)}…` : addr.slice(0, p2pIdx);
  const peerId = addr.slice(p2pIdx + 5);
  const short = peerId.length > 12 ? `${peerId.slice(0, 6)}…${peerId.slice(-6)}` : peerId;
  return `${prefix}/p2p/${short}`;
}
