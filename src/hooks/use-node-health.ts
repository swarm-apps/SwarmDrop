/**
 * useNodeHealth
 * 结论层那一句话 + 诊断层要用的逐条关系，供顶栏 pill / 节点状态面 / 设置页共用。
 *
 * # 为什么要自带一口时钟
 *
 * 宽限判据里有 `now - firstSeen >= 10s` 这一项，而后端只在**状态真的变了**的时候推。
 * 一条首拨失败的 link 会在 `Failed` 那一刻推一次，此时往往还在宽限内；若不自己走表，
 * 界面会永远停在「正在连接」，那 10 秒的宽限就变成了永久宽限。
 *
 * 时钟**只在还有待定判决时走**（有 link 处于 `settling`），落定后停——不是为了省 CPU，
 * 是为了不让一个稳定的界面每两秒重渲染一次。
 *
 * # 待定判决 ≠ 整体 `starting`
 *
 * 这里曾写作 `summary.level === "starting"`，两个方向都错：
 *
 * - **漏走表**：`resolveLevel` 在 `publicReachable` 为真时**先于** settling 判定就 return
 *   了。于是「两条引导节点连上一条」时整体是 `reachable`、定时器不起，`nowMs` 永远停在
 *   mount 那一刻，那条挂掉的 link 的 `pastGrace` 恒为 false —— 行头永远写着「正在连接」，
 *   而它的 `lastError` 就打印在同一张卡下面两行。后端也不会救场：`set_relay_state` 用
 *   `send_if_modified` 去重，状态稳定在 Failed 后不再推送。
 * - **空转**：节点在跑但 `infraLinks` 为空时整体恒为 `starting`（shared-view 里空清单也
 *   走这一档），定时器于是永不停止，顶栏 pill 每 2 秒白重渲染一次。
 *
 * 真正依赖 `nowMs` 的只有 `settling → unreachable` 这一次翻转，所以走表条件就是它本身。
 */

import { useEffect, useMemo, useState } from "react";
import type { NodeHealthSummary } from "@swarmdrop/shared-view";
import type { InfraLink, NetworkStatus } from "@/lib/bindings";
import {
  deriveInfraLinkState,
  summarizeNodeHealth,
  toInfraLinkView,
  toNodeStatusView,
} from "@/lib/node-status";
import { useNetworkStore, type NodeStatus } from "@/stores/network-store";

/** 待定判决时的走表间隔。宽限是 10s，2s 一格足够让翻转看起来是即时的。 */
const GRACE_TICK_MS = 2_000;

export interface NodeHealth {
  readonly summary: NodeHealthSummary;
  /** 前端生命周期态（`starting` / `error` 这两档健康度模型回答不了）。 */
  readonly lifecycle: NodeStatus;
  readonly links: InfraLink[];
  readonly networkStatus: NetworkStatus | null;
  /** 判据用的这一刻，诊断层逐条渲染要用同一个值，避免一屏里两套时间。 */
  readonly nowMs: number;
}

export function useNodeHealth(): NodeHealth {
  const lifecycle = useNetworkStore((s) => s.status);
  const networkStatus = useNetworkStore((s) => s.networkStatus);
  const [nowMs, setNowMs] = useState(() => Date.now());

  // selector 里不派生数组（会无限重渲染）——取出整份 status 再在这里 map。
  const links = useMemo(() => networkStatus?.infraLinks ?? [], [networkStatus]);
  const views = useMemo(() => links.map(toInfraLinkView), [links]);
  const statusView = useMemo(
    () => toNodeStatusView(networkStatus),
    [networkStatus],
  );
  const summary = useMemo(
    () => summarizeNodeHealth(statusView, views, nowMs),
    [statusView, views, nowMs],
  );

  const pending = useMemo(
    () =>
      views.some((v) => deriveInfraLinkState(v, nowMs).state === "settling"),
    [views, nowMs],
  );
  useEffect(() => {
    if (!pending) return;
    const id = setInterval(() => setNowMs(Date.now()), GRACE_TICK_MS);
    return () => clearInterval(id);
  }, [pending]);

  return { summary, lifecycle, links, networkStatus, nowMs };
}
