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
