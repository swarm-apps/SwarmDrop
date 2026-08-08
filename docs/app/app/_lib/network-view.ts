// 网络状态的**呈现层适配**：内核形状 → 共享判据入参 → 本端 catalog 的文案与色档。
//
// 判据本身不在这里（`@swarmdrop/shared-view` 的 `deriveInfraLinkState` /
// `summarizeNodeHealth`，三端同一份），这里只做三件本端才知道的事：
//
//   1. 形状转换   —— `InfraLink.firstSeen` 是 ISO 串，判据要毫秒数（uniffi 那端给的是 i64，
//                     所以转换归调用点，共享包里不设适配层）
//   2. 文案       —— 判据只返回 msgId，措辞归各端 catalog（DESIGN.md 的 Node Status Contract
//                     把 12 条 msgId 的中英文钉死，改这里的字要照那张表）
//   3. 色档       —— `StatusTone` → 本端 token 的类名
//
// **翻译宏在这里只能定义、不能展开**（`_lib/` 的既有约束）：所有文案存 `msg` 描述符，
// 由组件 `t(...)`。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import type {
  InfraLinkState,
  InfraLinkView,
  NodeHealthLevel,
  NodeStatusView,
  StatusTone,
} from "@swarmdrop/shared-view";
import type { NodeStatus } from "./store";
import type {
  BootstrapCandidateSource,
  CandidateScope,
  InfraAddrError,
  InfraLink,
} from "./view-types";

/**
 * 内核的 `InfraLink` → 共享判据的入参。
 *
 * **返回新对象，所以永远不要在 zustand selector 里调它**（规则 B：selector 派生新引用会
 * 无限重渲染）。调用点一律 `useMemo`。
 */
export function toInfraLinkView(link: InfraLink, nowMs: number): InfraLinkView {
  const firstSeen = Date.parse(link.firstSeen);
  return {
    roles: link.roles,
    scope: link.scope,
    // 解析不出来时**退回「现在」**而不是 0：后者会让这条 link 一进表就判定「过了宽限」，
    // 首次拨号还在飞就被宣布连不上。往安全方向退。
    firstSeenMs: Number.isNaN(firstSeen) ? nowMs : firstSeen,
    connected: link.connected,
    relay: link.relay,
    everActive: link.everActive,
    excluded: link.excluded,
  };
}

/**
 * 浏览器侧的 `NodeStatusView`。
 *
 * **Web 刻意不接 `NetworkStatus`**（决策 7）：那个聚合里的 `natStatus` 与 `discoveredPeers`
 * 在 wasm 下是结构性恒定值，接过来只会得到一排假状态。判据要的三个字段现搭：
 *
 * - `status`：本端的生命周期只有 `running` 一档算「在跑」，其余（idle / starting /
 *   closing / error）对判据都是「没在跑」；
 * - `publicReachable`：浏览器不 listen 任何 socket，「别人能不能拨到我」**等价于**
 *   「有没有一条活的 circuit 预留」。这不是近似，是这一端可达性的定义。
 */
export function toNodeStatusView(
  status: NodeStatus,
  circuitAddr: string | null,
  connectedPeers: number,
): NodeStatusView {
  return {
    status: status === "running" ? "running" : "stopped",
    publicReachable: circuitAddr !== null,
    connectedPeers,
  };
}

/** 色档 → 状态点类名。三端各用自己的 token，语义由共享判据给。 */
export const TONE_DOT: Record<StatusTone, string> = {
  neutral: "bg-muted-foreground",
  success: "bg-success",
  warning: "bg-warning",
};

/**
 * 单条关系的状态词（契约里 `infraLink.*` 那六条）。
 *
 * **按 `InfraLinkState` 建表而不是按 msgId 字符串**：两者一一对应，而枚举做键能让共享判据
 * 新增一档时这里编译期就红。msgId 写在每行注释里，方便与 DESIGN.md 的表对照。
 */
export const INFRA_LINK_STATE_LABEL: Record<InfraLinkState, MessageDescriptor> = {
  // infraLink.seedOnly
  seedOnly: msg`仅 DHT 种子`,
  // infraLink.excluded
  excluded: msg`已按设置排除`,
  // infraLink.settling
  settling: msg`正在连接`,
  // infraLink.ok
  ok: msg`已就绪`,
  // infraLink.lost
  lost: msg`连接已断`,
  // infraLink.unreachable
  unreachable: msg`连不上`,
};

/**
 * 结论层那**一句话**：可达性的后果句（契约里 `nodeHealth.*` 那六条）。
 *
 * 刻意都是完整句而不是「良好 / 受限」这类无主语形容词——后者说完等于没说，用户仍然不知道
 * 现在能不能收到别人发来的文件。
 */
export const NODE_HEALTH_MESSAGE: Record<NodeHealthLevel, MessageDescriptor> = {
  // nodeHealth.notRunning
  notRunning: msg`节点未运行`,
  // nodeHealth.starting
  starting: msg`正在连接网络…`,
  // nodeHealth.reachable
  reachable: msg`其他网络的设备可以连到你`,
  // nodeHealth.lanReachable
  lanReachable: msg`只有同一网络里的设备能连到你`,
  // nodeHealth.configuredLanOnly
  configuredLanOnly: msg`你关闭了公网可达性，其他网络的设备找不到你`,
  // nodeHealth.isolated
  isolated: msg`连不上任何网络，检查引导节点`,
};

/**
 * 徽章里那个**词**（契约结论层信息位 1：状态点 **和词**，光一个色点不满足）。
 *
 * 与上面的后果句是两个信息位，不是同一句话的长短两版：徽章宽度放不下一句话，而一句
 * 形容词又回答不了「所以我现在能不能被找到」。所以两者都要有，各就各位。
 */
export const NODE_HEALTH_WORD: Record<NodeHealthLevel, MessageDescriptor> = {
  notRunning: msg`未运行`,
  starting: msg`连接中`,
  reachable: msg`可达`,
  lanReachable: msg`仅局域网`,
  configuredLanOnly: msg`仅局域网`,
  isolated: msg`已孤立`,
};

/**
 * 归因：这条关系是怎么进到清单里来的。
 *
 * 用词照 DESIGN.md 的「网络概念 → 三端统一中文串」表——同一个概念在三端必须是同一个词，
 * 三份 catalog 此前已经漂过一轮（`Bootstrap Nodes` / 「公网引导」/「引导节点」并存）。
 */
export const INFRA_SOURCE_LABEL: Record<BootstrapCandidateSource, MessageDescriptor> = {
  hostConfigured: msg`手动配置`,
  mdnsLanHelper: msg`局域网协助`,
  learned: msg`自动发现`,
};

/** 地址范围。浏览器端 `lan` 恒不出现（没有 mDNS），分组自然为空，不特殊化。 */
export const INFRA_SCOPE_LABEL: Record<CandidateScope, MessageDescriptor> = {
  public: msg`公网`,
  lan: msg`局域网`,
};

/**
 * 这条关系承担的角色。
 *
 * **两个角色正交**（DHT 路由种子 / circuit 中继），本仓自建的那台恰好兼任——但分离部署时
 * 一台纯 DHT 种子完全可能出现，那时它没有 relay 轨道，也就没有失败态。
 */
export const INFRA_ROLE_LABEL = {
  kadServer: msg`DHT 种子`,
  relayServer: msg`中继`,
} as const satisfies Record<"kadServer" | "relayServer", MessageDescriptor>;

/**
 * 提交前校验的判别码 → 一句用户能据以行动的话。
 *
 * **不写 `default` 分支**：内核加一个变体，这里就该编译期红。兜底成「地址无效」等于把
 * 一条本来说得清的错误退化成一句无从下手的话。
 */
export function infraAddrErrorLabel(error: InfraAddrError): MessageDescriptor {
  switch (error.kind) {
    case "malformed":
      return msg`这不是一条合法的 multiaddr：${error.detail}`;
    case "missingPeerId":
      return msg`地址末尾缺少 /p2p/<节点 ID>，没有它就无法确认连上的是不是同一台机器`;
    case "noTransport":
      return msg`地址里没有可拨的传输段`;
    case "unsupportedTransport":
      // 浏览器只有 WebRTC 一族，粘一条 /tcp/ 进来是这一端最常见的错。带上本端支持什么，
      // 用户才知道下一步该找哪种地址。
      return msg`浏览器拨不了 ${error.transport} 地址，本端支持：${error.supported.join(" / ")}`;
    case "selfAddr":
      return msg`这是本机的地址`;
    case "duplicate":
      return msg`这条地址已经在清单里了`;
  }
}

/**
 * 把一个 reject 值收敛成 `InfraAddrError`，不是则返回 `null`。
 *
 * `infra_ensure` 的 reject 值恒是 `InfraAddrError`，但 JS 侧还可能撞上运行时异常，
 * 而两者都只是 `unknown`。判据是 `kind` 落在这六个里——它们与 `WebError.kind` 的七个
 * 取值**互不重叠**（Rust 侧两个枚举各自定义，重叠了这里会静默走错分支，所以两边都不许
 * 借用对方的判别码）。
 */
/**
 * 判别码全集。**是 `Record` 而不是手写的 `Set` 字面量**：后者写过一版，它与上面那个
 * 无 `default` 的 switch 只有一半的保护——core 加第七个变体时 switch 会红，这张表却会
 * 静默漏掉它，于是 `toInfraAddrError` 返回 `null`，一条本来说得清的新错误被路由进
 * 通用的「添加引导节点失败」并丢掉全部信息。正是上一段注释明令禁止的那种静默兜底。
 * 用枚举做键，两处一起红。
 */
const INFRA_ADDR_ERROR_KINDS: Record<InfraAddrError["kind"], true> = {
  malformed: true,
  missingPeerId: true,
  noTransport: true,
  unsupportedTransport: true,
  selfAddr: true,
  duplicate: true,
};

export function toInfraAddrError(e: unknown): InfraAddrError | null {
  if (e === null || typeof e !== "object" || !("kind" in e)) return null;
  const kind = (e as { kind: unknown }).kind;
  // `Object.hasOwn` 而不是 `in`：后者会顺着原型链把 `"toString"` 这类键判成命中。
  return typeof kind === "string" && Object.hasOwn(INFRA_ADDR_ERROR_KINDS, kind)
    ? (e as InfraAddrError)
    : null;
}
