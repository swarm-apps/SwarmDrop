/**
 * 网络状态文案与色档的**单一来源**（移动端这一份）。
 *
 * `@swarmdrop/shared-view` 的两个判据只返回 msgId 不返回文案，三端各自 catalog 里
 * 必须有同名条目；对应表写死在 `DESIGN.md` 的 Node Status Contract，改这里之前先读那张表。
 *
 * 四个屏（网络设置 / 引导节点 / 节点控制 sheet / 主屏 pill）都从这里取，不各写一份
 * ——三份 catalog 已经因为各写一份而漂过一次（「公网引导」vs「引导节点」）。
 *
 * 存的是 `msg` 描述符而不是 `<Trans>` 元素：同一条串既要渲染成文本、又要进
 * `accessibilityLabel`（那里只收字符串），描述符是唯一能同时喂两边的形态。
 */

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import type {
  InfraLinkState,
  InfraLinkView,
  NodeHealthCta,
  NodeHealthLevel,
  NodeHealthSummary,
  StatusTone,
} from "@swarmdrop/shared-view";
import type { CandidateSourceKey } from "@/core/network-discovery";
import type { RuntimeState } from "@/stores/mobile-core-store";

/** 结论层那句**后果句**——不是「良好 / 受限」这类无主语形容词。 */
export const NODE_HEALTH_SENTENCE: Record<NodeHealthLevel, MessageDescriptor> =
  {
    notRunning: msg`节点未运行`,
    starting: msg`正在连接网络…`,
    reachable: msg`其他网络的设备可以连到你`,
    lanReachable: msg`只有同一网络里的设备能连到你`,
    configuredLanOnly: msg`你关闭了公网可达性，其他网络的设备找不到你`,
    isolated: msg`连不上任何网络，检查引导节点`,
  };

/**
 * 常驻位那个**状态词**（pill 里跟着色点的那两三个字）。
 *
 * 与后果句分开是契约要求的两个信息位：色点 + 词回答「现在是什么状态」，后果句回答
 * 「这对我意味着什么」。一个光秃秃的色点不满足前者。
 */
export const NODE_HEALTH_WORD: Record<NodeHealthLevel, MessageDescriptor> = {
  notRunning: msg`未运行`,
  starting: msg`连接中`,
  reachable: msg`可达`,
  lanReachable: msg`仅局域网`,
  configuredLanOnly: msg`仅局域网`,
  // 「离线」说的是设备在线态，这一档说的是可达性——`DESIGN.md` 的网络词表把「在线」
  // 列为「可达」的废弃拼法，它的反义词同样不能用。桌面同一档就是「连不上」。
  isolated: msg`连不上`,
};

/** 结论层至多一个动作；`null` 是合法答案，不要为了对称造一个。 */
export const NODE_HEALTH_CTA_LABEL: Record<
  NonNullable<NodeHealthCta>,
  MessageDescriptor
> = {
  startNode: msg`启动节点`,
  openSettings: msg`去设置`,
  openDiagnostics: msg`查看诊断`,
};

/** 结论层的一格呈现：色档 + 状态词 + 后果句 + 至多一个动作。 */
export interface NodePresentation {
  readonly tone: StatusTone;
  /** 契约信息位 1：状态点旁边那个**词**（光有色点不算数）。 */
  readonly word: MessageDescriptor;
  /** 契约信息位 2：可达性**后果句**，不是「良好 / 受限」这类无主语形容词。 */
  readonly sentence: MessageDescriptor;
  readonly cta: NodeHealthCta;
}

/**
 * 节点生命周期里健康度**回答不了**的两个瞬时态。
 *
 * `summarizeNodeHealth` 说的是「起来之后连不连得上」，它把 `status !== "running"`
 * 一律折成 `notRunning`，所以 `starting` / `error` 借它的嘴说话就会自相矛盾：pill 说
 * 「启动中」、下面一行说「节点未运行」，还渲染出一颗点了会被 store 重入守卫吞掉的
 * 「启动节点」。这两态在健康度模型之外，所以在这里显式补，而不是硬塞进某一档。
 *
 * 用 `Record<RuntimeState, …>` 而不是 if 链：`RuntimeState` 加第五个态时这里编译期就红。
 * `error` 给警示档而非破坏档——破坏色在本仓语汇里表示「点了会丢数据」（停止节点那颗
 * 按钮），一个状态词不该占用它。桌面的孪生实现在 `src/lib/node-status.ts`。
 */
const LIFECYCLE_OVERRIDE: Record<RuntimeState, NodePresentation | null> = {
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

/** 把健康度与本端生命周期态合成一格呈现——**结论层的四个信息位只从这里取**。 */
export function resolveNodePresentation(
  lifecycle: RuntimeState,
  summary: NodeHealthSummary,
): NodePresentation {
  return (
    LIFECYCLE_OVERRIDE[lifecycle] ?? {
      tone: summary.tone,
      word: NODE_HEALTH_WORD[summary.level],
      sentence: NODE_HEALTH_SENTENCE[summary.level],
      cta: summary.cta,
    }
  );
}

/** 单条基础设施关系的状态词。 */
export const INFRA_LINK_LABEL: Record<InfraLinkState, MessageDescriptor> = {
  seedOnly: msg`仅 DHT 种子`,
  excluded: msg`已按设置排除`,
  settling: msg`正在连接`,
  ok: msg`已就绪`,
  lost: msg`连接已断`,
  unreachable: msg`连不上`,
};

/**
 * 候选来源。三个来源各占一条，**不留兜底**——`Learned` 曾被 `default` 折进
 * `hostConfigured`，于是运行时学到的节点被标成「配置节点」，归因整个是错的。
 */
export const CANDIDATE_SOURCE_LABEL: Record<
  CandidateSourceKey,
  MessageDescriptor
> = {
  hostConfigured: msg`配置节点`,
  mdnsLanHelper: msg`局域网协助`,
  learned: msg`自动发现`,
};

/** 候选范围。`DESIGN.md` 的网络词表：公网 / 局域网。 */
export const CANDIDATE_SCOPE_LABEL: Record<
  InfraLinkView["scope"],
  MessageDescriptor
> = {
  public: msg`公网`,
  lan: msg`局域网`,
};

/** 关系承担的角色（两个正交能力，可同时成立）。 */
export const INFRA_ROLE_LABEL = {
  kadServer: msg`DHT 种子`,
  relayServer: msg`中继`,
} satisfies Record<keyof InfraLinkView["roles"], MessageDescriptor>;

export const TONE_DOT_CLASS: Record<StatusTone, string> = {
  neutral: "bg-muted-foreground",
  success: "bg-success",
  warning: "bg-warning",
};

export const TONE_TEXT_CLASS: Record<StatusTone, string> = {
  neutral: "text-muted-foreground",
  success: "text-success-ink",
  warning: "text-warning-ink",
};

export const TONE_BG_CLASS: Record<StatusTone, string> = {
  neutral: "bg-muted",
  success: "bg-success/10",
  warning: "bg-warning/10",
};
