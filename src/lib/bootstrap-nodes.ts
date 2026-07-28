/**
 * 桌面端默认的引导/中继节点。
 *
 * 此清单是桌面 host 的部署配置，不属于跨平台 P2P 内核。仅在对应服务端实际公告
 * 地址后才把新 transport 加进来，避免客户端拨向不存在的 transport。
 *
 * **只列 TCP + QUIC**（与 `mobile/src/core/bootstrap-nodes.ts` 对齐）——原生端两者
 * 都直达公网。浏览器用不了 TCP/QUIC，它的清单
 * （`docs/app/app/_lib/relay-helpers.ts`）是 webrtc-direct。
 *
 * WebSocket 已于 2026-07-28 整体移除：它唯一的活（同网浏览器直连桌面）由
 * webrtc-direct 接管。
 */
export const DESKTOP_BOOTSTRAP_NODES: readonly string[] = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/** 合并桌面默认与用户配置，保留首次出现的地址顺序。 */
export function getDesktopBootstrapNodes(customNodes: readonly string[]): string[] {
  return [...new Set([...DESKTOP_BOOTSTRAP_NODES, ...customNodes])];
}
