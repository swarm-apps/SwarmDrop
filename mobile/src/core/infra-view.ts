/**
 * 原生 `MobileInfraLink` → `@swarmdrop/shared-view` 判据入参的适配。
 *
 * **转换归调用点**是 shared-view 的既定约定（那个包不设适配层，否则「哪一端叫什么」
 * 会漏进平台中立的代码）。移动端要转的有三样：uniffi 的 `bigint` 时间戳、判别式从
 * `tag` / `inner` 形态换成 `kind` 形态、以及两个数字枚举换成 wire 名。
 *
 * 判定本身一律**不在这里**做——它住在 `deriveInfraLinkState` / `summarizeNodeHealth`，
 * 三端共用一份。这里只负责搬运。
 */

import type {
  InfraLinkView,
  InfraRelayState,
  NodeStatusView,
} from "@swarmdrop/shared-view";
import {
  MobileCandidateScope,
  MobileInfraExclusion,
  type MobileInfraLink,
  type MobileNetworkStatus,
  type MobileRelayLinkState,
  MobileRelayLinkState_Tags,
} from "react-native-swarmdrop-core";

const SCOPE: Record<MobileCandidateScope, InfraLinkView["scope"]> = {
  [MobileCandidateScope.Public]: "public",
  [MobileCandidateScope.Lan]: "lan",
};

/**
 * `InfraExclusion` 的 wire 名。
 *
 * shared-view 把 `kind` 声明成开放的 `string` 并把未知值当「被设置排除」处理，
 * 所以这里只需如实搬名字；后端新增变体时这张表会因缺键而编译期报错。
 */
const EXCLUSION_KIND: Record<MobileInfraExclusion, string> = {
  [MobileInfraExclusion.PublicReachabilityDisabled]:
    "publicReachabilityDisabled",
};

function toRelayState(
  relay: MobileRelayLinkState | undefined,
): InfraRelayState | null {
  // `undefined` = 这条关系在内核里没有 relay 轨道，不是「状态未知」。
  if (!relay) return null;
  // 三个 tag 逐一列出、**不留 default**：内核加一态时这里编译期红。
  switch (relay.tag) {
    case MobileRelayLinkState_Tags.Connecting:
      return { kind: "connecting" };
    case MobileRelayLinkState_Tags.Active:
      return { kind: "active", circuitAddr: relay.inner.circuitAddr };
    case MobileRelayLinkState_Tags.Failed:
      return { kind: "failed", lastError: relay.inner.lastError };
  }
}

export function toInfraLinkView(link: MobileInfraLink): InfraLinkView {
  return {
    roles: {
      relayServer: link.roles.relayServer,
      kadServer: link.roles.kadServer,
    },
    scope: SCOPE[link.scope],
    firstSeenMs: Number(link.firstSeen),
    connected: link.connected,
    relay: toRelayState(link.relay),
    everActive: link.everActive,
    excluded:
      link.excluded === undefined
        ? null
        : { kind: EXCLUSION_KIND[link.excluded] },
  };
}

/**
 * 结论层判据的入参。
 *
 * `status` 取的是 **store 的 `runtimeState`** 而不是 `networkStatus.status`：后者在
 * `starting` / `error` 期间是上一轮的残留快照，用它会让「正在启动」被读成「在跑」。
 */
export function toNodeStatusView(
  runtimeState: string,
  status: MobileNetworkStatus | null,
): NodeStatusView {
  return {
    status: runtimeState,
    publicReachable: status?.publicReachable ?? false,
    connectedPeers: Number(status?.connectedPeers ?? 0),
  };
}

/**
 * 从 multiaddr 里取出身份，供 UI 把配置清单与 `infraLinks` 对上，以及作为移除的键。
 *
 * 取**最后一个** `/p2p/` 段——与内核 `Addr::p2p_node_id` 同口径：circuit 地址
 * `…/p2p/<中继>/p2p-circuit/p2p/<目标>` 说的是目标而不是那一跳。
 */
export function peerIdFromMultiaddr(addr: string): string | null {
  const segments = addr.split("/p2p/");
  if (segments.length < 2) return null;
  const last = segments[segments.length - 1]?.split("/")[0]?.trim();
  return last ? last : null;
}
