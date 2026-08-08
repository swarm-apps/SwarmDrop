import { useMemo } from "react";

import { useNetworkStore } from "@/stores/network-store";

/**
 * 已配对**且**在线的设备数——契约结论层信息位 3 里那个「在线 M」。
 *
 * 抽成一个 hook 是因为它有两个消费者（节点状态面与设置页的设备信息格），而它们必须同源：
 * 同一个概念在同一个应用里出现两个数，用户只会认为其中一个是坏的。
 *
 * **不要改回 `networkStatus.connectedPeers`。** 后端那个数是
 * `DeviceManager::connected_count()`——「所有 agent_version 认得出是 SwarmDrop 的已连对端」，
 * 与配对无关。局域网协助节点本身就是另一台 SwarmDrop 桌面，于是零配对设备在线的用户会看到
 * 「已连接设备 1」，点进设备页却空空如也。`connected_peers` 字段本身是对的，只是它回答的
 * 是另一个问题（另有 e2e 与 MCP 面消费者），不该拿来填这一格。
 */
export function usePairedOnlineCount(): number {
  const devices = useNetworkStore((s) => s.devices);
  // selector 里不派生（会无限重渲染），计数放 useMemo。
  return useMemo(
    () => devices.filter((d) => d.status === "online" && d.isPaired).length,
    [devices],
  );
}
