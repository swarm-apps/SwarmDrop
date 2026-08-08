/**
 * 节点健康与逐条基础设施关系的读取入口。
 *
 * 判定全部来自 `@swarmdrop/shared-view`（三端同一份），本 hook 只做两件事：
 * 把原生形状适配成判据入参、以及在**宽限窗口到期时**推一次重渲染。
 */

import {
  deriveInfraLinkState,
  INFRA_GRACE_MS,
  type InfraLinkPresentation,
  type InfraLinkView,
  type NodeHealthSummary,
  summarizeNodeHealth,
} from "@swarmdrop/shared-view";
import { useEffect, useMemo, useState } from "react";
import type { MobileInfraLink } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { toInfraLinkView, toNodeStatusView } from "@/core/infra-view";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

/** 一条关系：原生数据 + 判定结果，UI 直接渲染。 */
export interface InfraLinkRow {
  readonly link: MobileInfraLink;
  readonly view: InfraLinkView;
  readonly presentation: InfraLinkPresentation;
}

export interface NodeHealth {
  readonly summary: NodeHealthSummary;
  readonly rows: readonly InfraLinkRow[];
}

/**
 * 判据里唯一会「自己到期」的东西是宽限窗口（`firstSeen + 10s`）。
 *
 * 所以这里**不设常驻定时器**：只在还有 link 处在窗口内时安排一次唤醒，全部过期后
 * 停表。主屏的状态 pill 也吃这个 hook，每秒重渲染会一路带动整张设备列表。
 */
function useGraceAwareNow(links: readonly InfraLinkView[]): number {
  const [nowMs, setNowMs] = useState(() => Date.now());

  const nextDeadline = links.reduce<number | null>((earliest, link) => {
    const at = link.firstSeenMs + INFRA_GRACE_MS;
    if (at <= nowMs) return earliest;
    return earliest === null || at < earliest ? at : earliest;
  }, null);

  useEffect(() => {
    if (nextDeadline === null) return;
    // +50ms 余量：踩着边界唤醒会因为 `>=` 差一毫秒而空转一轮。
    const delay = Math.max(nextDeadline - Date.now(), 0) + 50;
    const id = setTimeout(() => setNowMs(Date.now()), delay);
    return () => clearTimeout(id);
  }, [nextDeadline]);

  return nowMs;
}

export function useNodeHealth(): NodeHealth {
  const { runtimeState, networkStatus } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      networkStatus: s.networkStatus,
    })),
  );

  const links = useMemo(
    () => networkStatus?.infraLinks ?? [],
    [networkStatus?.infraLinks],
  );
  const views = useMemo(() => links.map(toInfraLinkView), [links]);
  const nowMs = useGraceAwareNow(views);

  return useMemo(() => {
    const rows = links.map((link, index) => {
      const view = views[index] as InfraLinkView;
      return {
        link,
        view,
        presentation: deriveInfraLinkState(view, nowMs),
      };
    });
    return {
      summary: summarizeNodeHealth(
        toNodeStatusView(runtimeState, networkStatus),
        views,
        nowMs,
      ),
      rows,
    };
  }, [links, views, nowMs, runtimeState, networkStatus]);
}
