/**
 * 移动端默认的引导/中继节点。
 *
 * Android 当前没有 WebSocket transport，因此这里仅配置 TCP/QUIC。移动端与桌面端
 * 的节点清单刻意分离，服务端新增可用 transport 后可独立调整。
 */
export const MOBILE_BOOTSTRAP_NODES: readonly string[] = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/** 合并移动默认与用户配置，保留首次出现的地址顺序。 */
export function getMobileBootstrapNodes(customNodes: readonly string[]): string[] {
  return [...new Set([...MOBILE_BOOTSTRAP_NODES, ...customNodes])];
}
