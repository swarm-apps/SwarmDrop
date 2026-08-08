"use client";

// 网络状态判据的 React 接线：store 的原始字段 → 共享判据 → 组件直接可渲染的结果。
//
// 收在一处而不是各组件内联，理由与 `selectOfferCount` 那批 selector 相同：**判据只该有一个
// 入口**。此前「节点健康不健康」在三个地方各有一套土办法（徽章看 `status`、弹窗数 active
// relay、面板逐条看 state），于是同一屏上会同时出现「运行中（绿）」和一整列失败的中继。
//
// 两个 hook 都在 `useMemo` 里做派生，**不在 selector 里**——`pnpm check:zustand-access`
// 的规则 B 覆盖本目录，selector 返回新数组/对象会打穿 `useSyncExternalStore`。

import { useMemo } from "react";
import {
  deriveInfraLinkState,
  summarizeNodeHealth,
  type InfraLinkPresentation,
  type NodeHealthSummary,
} from "@swarmdrop/shared-view";
import { toInfraLinkView, toNodeStatusView } from "./network-view";
import { selectReservation, useWebNode } from "./store";
import { useNowSeconds } from "./use-now-seconds";
import type { InfraLink } from "./view-types";

/** 一条基础设施关系 + 它算出来的呈现态。 */
export interface InfraLinkRow {
  readonly link: InfraLink;
  readonly presentation: InfraLinkPresentation;
}

/**
 * 逐条关系的呈现态。
 *
 * 时间基准走共享节拍（30s），而不是各自 `Date.now()`：同屏的几条要读同一个「现在」，
 * 否则相邻两行会在不同时刻跨过宽限线。**宽限的真正锚点是「已观测到至少一次 failed」**，
 * 而那是 relay 事件推来的，会自带一次重渲染——节拍只负责兜住「一直没有新事件」的那段。
 */
export function useInfraLinkRows(): InfraLinkRow[] {
  const links = useWebNode((s) => s.infraLinks);
  const nowSec = useNowSeconds();
  return useMemo(() => {
    const nowMs = nowSec * 1000;
    return links.map((link) => ({
      link,
      presentation: deriveInfraLinkState(toInfraLinkView(link, nowMs), nowMs),
    }));
  }, [links, nowSec]);
}

/**
 * 整体网络健康——结论层那一句话。
 *
 * 这是「节点在跑但全部中继都挂了，徽章却还是绿的」那个缺陷的修法：判据不再只看
 * `status`，公网可达性由 circuit 预留回答（浏览器不 listen socket，这就是它的可达性定义）。
 */
export function useNodeHealth(): NodeHealthSummary {
  const status = useWebNode((s) => s.status);
  const links = useWebNode((s) => s.infraLinks);
  const connectedPeers = useWebNode((s) => s.connectedPeers);
  const circuitAddr = useWebNode(selectReservation);
  const nowSec = useNowSeconds();

  return useMemo(() => {
    const nowMs = nowSec * 1000;
    return summarizeNodeHealth(
      toNodeStatusView(status, circuitAddr, connectedPeers),
      links.map((link) => toInfraLinkView(link, nowMs)),
      nowMs,
    );
  }, [status, links, connectedPeers, circuitAddr, nowSec]);
}
