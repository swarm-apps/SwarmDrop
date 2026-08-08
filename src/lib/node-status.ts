/**
 * 节点状态的桌面呈现层：把 `@swarmdrop/shared-view` 的两个判据接到桌面的 catalog 与色档。
 *
 * 判据本身（`deriveInfraLinkState` / `summarizeNodeHealth`）**不在这里**——它们是三端共享
 * 的，只返回 msgId。本模块负责三件事，全都是「本端怎么渲染」：
 *
 * 1. bindings 的 `InfraLink` → 判据的 `InfraLinkView`（唯一的差是时间形状）；
 * 2. msgId → 桌面 catalog 的 `MessageDescriptor`（12 条，文案由 `DESIGN.md` 的
 *    Node Status Contract 钉死，三端必须同义）；
 * 3. 色档 → Tailwind token 类。
 *
 * **只存 `msg` 描述符，不在这里展开。** 展开要 `t(...)`，那是组件的事（本仓既有约定，
 * 见 CLAUDE.md 的「翻译宏只在组件里展开」）。
 */

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import {
  deriveInfraLinkState,
  summarizeNodeHealth,
  type InfraLinkState,
  type InfraLinkView,
  type NodeHealthCta,
  type NodeHealthLevel,
  type NodeHealthSummary,
  type NodeStatusView,
  type StatusTone,
} from "@swarmdrop/shared-view";
import type { InfraAddrError, InfraLink, NetworkStatus } from "@/lib/bindings";
import type { NodeStatus } from "@/stores/network-store";

export type { InfraLinkState, NodeHealthCta, NodeHealthLevel, StatusTone };
export { deriveInfraLinkState, summarizeNodeHealth };

/**
 * bindings 的 `InfraLink` → 判据入参。
 *
 * 唯一要转的是时间：specta 把 `DateTime<Utc>` 导成 ISO 串，判据要毫秒。
 * 转换归调用点是 shared-view 的既定边界（那个包不设适配层，否则「哪一端叫什么」
 * 会渗进平台中立的包）。
 */
export function toInfraLinkView(link: InfraLink): InfraLinkView {
  return {
    roles: link.roles,
    scope: link.scope,
    firstSeenMs: Date.parse(link.firstSeen),
    connected: link.connected,
    relay: link.relay,
    everActive: link.everActive,
    excluded: link.excluded,
  };
}

/** `NetworkStatus` → 判据入参。节点没跑时后端根本不下发状态，`null` 即「未运行」。 */
export function toNodeStatusView(status: NetworkStatus | null): NodeStatusView {
  return {
    status: status?.status ?? "stopped",
    publicReachable: status?.publicReachable ?? false,
    connectedPeers: status?.connectedPeers ?? 0,
  };
}

/** 结论层的一格呈现：色档 + 状态词 + 后果句 + 至多一个动作。 */
export interface NodePresentation {
  readonly tone: StatusTone;
  /** 结论层槽 1：状态点旁边那个**词**（光有色点不满足契约）。 */
  readonly word: MessageDescriptor;
  /** 结论层槽 2：可达性**后果句**，不是「良好 / 受限」这类无主语形容词。 */
  readonly sentence: MessageDescriptor;
  readonly cta: NodeHealthCta;
}

/**
 * 六档健康度的桌面呈现。
 *
 * `sentence` 逐条对应 `DESIGN.md` 契约表里的 msgId——**改这里必须同改另外三份
 * catalog**（Web / 移动 / 托盘 rust-i18n），那张表就是为了让四份不漂移才存在的。
 * 显式 id 让四端的同一句话在 .po 里也叫同一个名字。
 */
const HEALTH: Record<NodeHealthLevel, Omit<NodePresentation, "cta">> = {
  notRunning: {
    tone: "neutral",
    word: msg`未启动`,
    sentence: msg({ id: "nodeHealth.notRunning", message: "节点未运行" }),
  },
  starting: {
    tone: "neutral",
    word: msg`连接中`,
    sentence: msg({ id: "nodeHealth.starting", message: "正在连接网络…" }),
  },
  reachable: {
    tone: "success",
    word: msg`公网可达`,
    sentence: msg({
      id: "nodeHealth.reachable",
      message: "其他网络的设备可以连到你",
    }),
  },
  lanReachable: {
    tone: "neutral",
    word: msg`仅局域网`,
    sentence: msg({
      id: "nodeHealth.lanReachable",
      message: "只有同一网络里的设备能连到你",
    }),
  },
  configuredLanOnly: {
    tone: "neutral",
    word: msg`仅局域网`,
    sentence: msg({
      id: "nodeHealth.configuredLanOnly",
      message: "你关闭了公网可达性，其他网络的设备找不到你",
    }),
  },
  isolated: {
    tone: "warning",
    word: msg`连不上`,
    sentence: msg({
      id: "nodeHealth.isolated",
      message: "连不上任何网络，检查引导节点",
    }),
  },
};

/**
 * 节点生命周期里健康度**回答不了**的两个瞬时态。
 *
 * `summarizeNodeHealth` 说的是「起来之后连不连得上」，它没有「还没起来」与「起失败了」
 * 这两档：前端 store 的 `starting` 是启动命令在飞，`error` 是启动失败。两者都在
 * 健康度模型之外，所以在这里显式补，而不是把它们硬塞进某一档。
 *
 * 用 `Record<NodeStatus, …>` 而不是 if 链：`NodeStatus` 加第五个态时这里编译期就红。
 * `error` 给警示档而非破坏档——破坏色在本仓语汇里表示「点了会丢数据」（停止节点那颗
 * 按钮），一个状态词不该占用它。
 */
const LIFECYCLE_OVERRIDE: Record<NodeStatus, NodePresentation | null> = {
  stopped: null,
  running: null,
  starting: {
    tone: "neutral",
    word: msg`启动中`,
    sentence: msg`正在启动节点…`,
    cta: null,
  },
  error: {
    tone: "warning",
    word: msg`节点错误`,
    sentence: msg`节点启动失败，重试或查看日志`,
    cta: null,
  },
};

/** 把健康度与前端生命周期态合成一格呈现。 */
export function resolveNodePresentation(
  lifecycle: NodeStatus,
  summary: NodeHealthSummary,
): NodePresentation {
  return (
    LIFECYCLE_OVERRIDE[lifecycle] ?? {
      ...HEALTH[summary.level],
      cta: summary.cta,
    }
  );
}

/**
 * 单条关系的六个状态词。
 *
 * `seedOnly` / `excluded` 是中性档且**没有失败态**——前者在内核里没有 relay 轨道，
 * 后者是用户自己关的闸门。给它们红点会训练用户忽略状态色。
 */
export const INFRA_LINK_WORD: Record<InfraLinkState, MessageDescriptor> = {
  seedOnly: msg({ id: "infraLink.seedOnly", message: "仅 DHT 种子" }),
  excluded: msg({ id: "infraLink.excluded", message: "已按设置排除" }),
  settling: msg({ id: "infraLink.settling", message: "正在连接" }),
  ok: msg({ id: "infraLink.ok", message: "已就绪" }),
  lost: msg({ id: "infraLink.lost", message: "连接已断" }),
  unreachable: msg({ id: "infraLink.unreachable", message: "连不上" }),
};

/** 候选来源 → 标签。加第四个来源时这里编译期就红。 */
export const INFRA_SOURCE_LABEL: Record<
  InfraLink["sources"][number],
  MessageDescriptor
> = {
  hostConfigured: msg`配置节点`,
  mdnsLanHelper: msg`局域网协助`,
  learned: msg`自动发现`,
};

/**
 * 地址范围 → 标签。
 *
 * 它不是装饰：`summarizeNodeHealth` 的 `configuredLanOnly` 那一档就是按 `scope == public`
 * 判的，用户看不到这一格，就没法把「我关了公网可达性」和眼前这条中性行对上。
 */
export const INFRA_SCOPE_LABEL: Record<
  InfraLink["scope"],
  MessageDescriptor
> = {
  public: msg`公网`,
  lan: msg`局域网`,
};

/**
 * 这条关系承担的角色。
 *
 * **两个角色正交**（DHT 路由种子 / circuit 中继），所以归因里它是**列举**不是二选一——
 * 本仓自建的那台恰好兼任，但分离部署时一台纯 DHT 种子完全可能出现，那时它没有 relay
 * 轨道，也就没有失败态（即 `seedOnly` 那一档）。
 */
export const INFRA_ROLE_LABEL: Record<
  keyof InfraLink["roles"],
  MessageDescriptor
> = {
  kadServer: msg`DHT 种子`,
  relayServer: msg`中继`,
};

/** 色档 → 状态点的类。 */
export const TONE_DOT: Record<StatusTone, string> = {
  neutral: "bg-muted-foreground",
  success: "bg-success",
  warning: "bg-warning",
};

/** 色档 → 徽标底 + 文字的类。 */
export const TONE_BADGE: Record<StatusTone, string> = {
  neutral: "bg-muted text-muted-foreground",
  success: "bg-success/15 text-success-ink",
  warning: "bg-warning/15 text-warning-ink",
};

/**
 * 提交前校验被拒的原因 → 文案。
 *
 * 键是后端 `InfraAddrError` 的判别码，**按变体渲染而不是匹配错误串**：后端加一个
 * 变体，这里编译期就红。`unsupportedTransport` 的两个字段（地址里那个传输、本端支持
 * 什么）由调用点填进占位——错误自带它们，正是为了让 UI 说得出下一步。
 */
export const INFRA_ADDR_ERROR_TITLE: Record<
  InfraAddrError["kind"],
  MessageDescriptor
> = {
  malformed: msg`不是合法的 Multiaddr 地址`,
  missingPeerId: msg`地址里缺少 /p2p/ 节点身份`,
  noTransport: msg`地址里没有可拨的传输段`,
  unsupportedTransport: msg`本端点不支持这种传输`,
  selfAddr: msg`这是本机的地址`,
  duplicate: msg`该引导节点已存在`,
};
