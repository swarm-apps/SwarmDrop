/**
 * 整体网络健康：结论层那**一句话**。
 *
 * 它回答的是「别人现在能不能连到我」，不是「几条引导节点连上了」。后者是诊断层的
 * 事实——两条引导节点连上一条，后果与连上两条完全一样，所以 `1/2` **不得**让常驻位
 * 报警（否则就是在训练用户忽略状态色）。
 *
 * 可达性本身**不在这里判**：规则含 `CandidateScope` 这条领域知识，且 `crates/web` 是
 * 第四个消费者（生成邀请时判本机可达），TS 包救不到它。本函数消费的是 Rust 已经算好的
 * `publicReachable`，只负责把它与 link 状态合成一句话。推导见
 * `openspec/changes/unify-infra-node-status/design.md` 决策 6 与 10。
 *
 * ⚠️ **本文件有一个必须同步的孪生实现：`src-tauri/src/node_health.rs`。**
 * 托盘是第四个表现层，整条在 Rust 里、文案走 rust-i18n，够不到这个包。它逐档照抄本文件的
 * 分档顺序、宽限三条件与 `INFRA_GRACE_MS`，并有自己的单测钉住同义关系——**改这里的分档
 * 必须同改那边**。为什么不反过来「让 Rust 算好档位随 `NetworkStatus` 下发、TS 只渲染」：
 * `settling → unreachable` 的翻转由时间驱动而非事件驱动，推送模型里那个档位一推出去就在
 * 变旧，客户端必须能自己按 `nowMs` 重算。
 */

import { deriveInfraLinkState, type StatusTone } from "./infra-link";
import type { InfraLinkView, NodeStatusView } from "./types";

export type NodeHealthLevel =
  | "notRunning"
  | "starting"
  | "reachable"
  | "lanReachable"
  | "configuredLanOnly"
  | "isolated";

/**
 * 结论层至多**一个**动作。`null` = 这一态没有可点的东西，不要为了对称造一个。
 */
export type NodeHealthCta =
  | "startNode"
  | "openSettings"
  | "openDiagnostics"
  | null;

export interface NodeHealthSummary {
  readonly level: NodeHealthLevel;
  readonly tone: StatusTone;
  /** 结论层文案的 msgId。三端各自 catalog 里必须有同名条目。 */
  readonly msgId: string;
  readonly cta: NodeHealthCta;
}

const SUMMARY: Record<NodeHealthLevel, Omit<NodeHealthSummary, "level">> = {
  notRunning: {
    tone: "neutral",
    msgId: "nodeHealth.notRunning",
    cta: "startNode",
  },
  starting: { tone: "neutral", msgId: "nodeHealth.starting", cta: null },
  reachable: { tone: "success", msgId: "nodeHealth.reachable", cta: null },
  // 中性而非警示：对多数家用用户，同网可达完全够用，报警是在制造焦虑。
  lanReachable: { tone: "neutral", msgId: "nodeHealth.lanReachable", cta: null },
  configuredLanOnly: {
    tone: "neutral",
    msgId: "nodeHealth.configuredLanOnly",
    cta: "openSettings",
  },
  isolated: {
    tone: "warning",
    msgId: "nodeHealth.isolated",
    cta: "openDiagnostics",
  },
};

/**
 * 合成结论层。
 *
 * 分档顺序即优先级：
 *
 * 1. **节点没跑**压倒一切——今天移动端把它误报成「公网引导节点未连接」，用户去查了
 *    一圈引导节点，而真正该点的是「启动节点」。
 * 2. **公网可达**是成功档的唯一判据（Rust 已算好）。
 * 3. **用户自己关的闸门**排在「正在连接」之前：那些 link 永远不会 settle，等下去毫无
 *    意义，且这一档的 CTA（去设置）才是有效动作。它也压过 `lanReachable`——两句话说的
 *    是同一件事，但只有这一句能告诉用户为什么以及去哪改。
 * 4. **还有 link 在收敛**（或一条 link 都还没进表）→ 安静地等。
 * 5. 有已连对端 → 同网可达。
 * 6. 剩下的才是孤立。
 *
 * 报警三条件（非配置造成 ∧ 过宽限 ∧ 确实挡住了用户此刻的动作）只在第 6 档同时成立。
 */
export function summarizeNodeHealth(
  status: NodeStatusView,
  links: readonly InfraLinkView[],
  nowMs: number,
): NodeHealthSummary {
  const level = resolveLevel(status, links, nowMs);
  return { level, ...SUMMARY[level] };
}

function resolveLevel(
  status: NodeStatusView,
  links: readonly InfraLinkView[],
  nowMs: number,
): NodeHealthLevel {
  if (status.status !== "running") return "notRunning";
  if (status.publicReachable) return "reachable";

  const publicLinks = links.filter((l) => l.scope === "public");
  if (publicLinks.length > 0 && publicLinks.every((l) => l.excluded !== null)) {
    return "configuredLanOnly";
  }

  // 空清单也走这一档：没有 link 就没有可诊断的对象，此时喊「检查引导节点」是指着一张
  // 空表说话。真的零配置时诊断层里那张空表本身就是答案，不需要常驻位红着。
  const settling = links.some(
    (l) => deriveInfraLinkState(l, nowMs).state === "settling",
  );
  if (settling || links.length === 0) return "starting";

  if (status.connectedPeers > 0) return "lanReachable";
  return "isolated";
}
