/**
 * 桌面端默认的引导/中继节点。
 *
 * 此清单是桌面 host 的部署配置，不属于跨平台 P2P 内核。仅在对应服务端实际公告
 * 地址后才把新 transport 加进来，避免客户端拨向不存在的 transport。
 *
 * **只列 TCP + QUIC**（与 `mobile/src/core/bootstrap-nodes.ts` 对齐）。原生端两者
 * 都直达公网，服务端的 `/tcp/4002/ws` 永远排不上号；而浏览器也用不了它——https
 * 页面拨公网裸 IP 的 `ws://` 会被 mixed content 拦，Web 端清单
 * （`docs/app/try/relay-helpers.ts`）因此只有 webrtc-direct。
 *
 * 注意这不代表 WebSocket 在桌面无用：桌面**自身**仍监听 `/ws`，那是同网浏览器
 * 直连本机的入口（`crates/net/src/endpoint/presets.rs`），与本清单无关。
 */
export const DESKTOP_BOOTSTRAP_NODES: readonly string[] = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep",
];

/** 合并桌面默认与用户配置，保留首次出现的地址顺序。 */
export function getDesktopBootstrapNodes(customNodes: readonly string[]): string[] {
  return [...new Set([...DESKTOP_BOOTSTRAP_NODES, ...customNodes])];
}
