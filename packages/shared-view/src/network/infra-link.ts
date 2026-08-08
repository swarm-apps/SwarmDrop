/**
 * 单条基础设施关系的呈现判据。
 *
 * 状态机与宽限规则的完整推导见 `openspec/changes/unify-infra-node-status/design.md`
 * 决策 10；三端必须一致的部分写在 `DESIGN.md` 的 Node Status Contract。
 *
 * 本模块**只返回 msgId 不返回文案**（README 判据 3 的推论）——三端各有各的 catalog，
 * 把中文烤进返回值等于给这个包同时安上格式化器和翻译源两种身份。
 */

import type { InfraLinkView } from "./types";

/** 状态色档。三端的具体颜色由各自的 token 决定，这里只说语义。 */
export type StatusTone = "neutral" | "success" | "warning";

/**
 * 单条关系的呈现态。
 *
 * `seedOnly` 与 `excluded` 都**没有失败态**：前者在内核里没有 relay 轨道，后者是
 * 用户自己关的闸门。给它们红点会训练用户忽略状态色。
 */
export type InfraLinkState =
  | "seedOnly"
  | "excluded"
  | "settling"
  | "ok"
  | "lost"
  | "unreachable";

export interface InfraLinkPresentation {
  readonly state: InfraLinkState;
  readonly tone: StatusTone;
  /** 状态词的 msgId。三端各自 catalog 里必须有同名条目。 */
  readonly msgId: string;
  /**
   * 需要原样展示的诊断串（`last_error`），无则 `null`。
   *
   * **不翻译**：排查时用户要贴的就是这一句，翻过的串贴进 issue 反而没用。
   */
  readonly detail: string | null;
}

/**
 * 首次连接的宽限窗口。
 *
 * 它只在**三条件同时成立**时生效（见 `deriveInfraLinkState`），所以这个数并不是
 * 「多久之后才报错」——真正的锚是「已经观测到至少一次 Failed」。
 */
export const INFRA_GRACE_MS = 10_000;

const PRESENTATION: Record<InfraLinkState, { tone: StatusTone; msgId: string }> =
  {
    seedOnly: { tone: "neutral", msgId: "infraLink.seedOnly" },
    excluded: { tone: "neutral", msgId: "infraLink.excluded" },
    settling: { tone: "neutral", msgId: "infraLink.settling" },
    ok: { tone: "success", msgId: "infraLink.ok" },
    lost: { tone: "warning", msgId: "infraLink.lost" },
    unreachable: { tone: "warning", msgId: "infraLink.unreachable" },
  };

/**
 * 把一条 `InfraLink` 判成呈现态。
 *
 * 分档顺序即优先级，逐条都有理由：
 *
 * 1. `excluded` 最先——它说的是**设置**不是故障，任何故障态盖过它都会把「你自己关的」
 *    渲染成「坏了」，把用户领向重试而不是那个开关。
 * 2. `seedOnly` 次之——纯 DHT 种子在内核里没有 relay 轨道，`relay == null` 对它是
 *    正常而不是「状态未知」。
 * 3. 有 relay 轨道之后才谈成功/失败。
 *
 * **宽限三条件缺一不可**：`!everActive` ∧ 已见 `failed` ∧ `now - firstSeen >= GRACE`。
 * 只看时间会在首次拨号还在飞的时候就宣布失败（native 拨号超时 30s 远大于任何合理的
 * 宽限）；只看「已见 failed」则首次偶发失败、第二次 2s 后成功时会闪一下红。
 */
export function deriveInfraLinkState(
  link: InfraLinkView,
  nowMs: number,
): InfraLinkPresentation {
  const detail =
    link.relay?.kind === "failed" ? (link.relay.lastError ?? null) : null;
  const present = (state: InfraLinkState): InfraLinkPresentation => ({
    state,
    ...PRESENTATION[state],
    detail: state === "seedOnly" || state === "excluded" ? null : detail,
  });

  if (link.excluded) return present("excluded");
  if (!link.roles.relayServer) return present("seedOnly");
  if (link.relay?.kind === "active") return present("ok");

  // 曾经连上过就不吃宽限：用户已经见过它是好的，掉线要立刻说。
  if (link.everActive) return present("lost");

  const sawFailure = link.relay?.kind === "failed";
  const pastGrace = nowMs - link.firstSeenMs >= INFRA_GRACE_MS;
  if (sawFailure && pastGrace) return present("unreachable");

  // `relay == null` 落到这里读作「有角色但内核还没建轨道」——收敛环最迟 1s 起第一轮。
  return present("settling");
}
