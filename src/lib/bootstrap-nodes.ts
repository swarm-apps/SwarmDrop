/**
 * 桌面端默认的引导/中继节点。
 *
 * 此清单是桌面 host 的部署配置，不属于跨平台 P2P 内核。桌面支持 TCP、QUIC、
 * WebSocket 和 WebRTC Direct；仅在对应服务端实际公告地址后才把 /ws 或
 * /webrtc-direct 地址加入这里，避免客户端拨向不存在的 transport。
 */
export const DESKTOP_BOOTSTRAP_NODES: readonly string[] = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
  "/ip4/47.115.172.218/tcp/4002/ws/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/** 合并桌面默认与用户配置，保留首次出现的地址顺序。 */
export function getDesktopBootstrapNodes(customNodes: readonly string[]): string[] {
  return [...new Set([...DESKTOP_BOOTSTRAP_NODES, ...customNodes])];
}
