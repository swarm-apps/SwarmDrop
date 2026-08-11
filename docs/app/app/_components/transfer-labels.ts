// 传输活动视图的**标签表与纯函数**。从 `transfer-activity-panel.tsx` 抽出来的——
// 那个文件曾经在一处同时装着 5 张标签表、4 个纯函数与 8 个组件。
//
// 这一层的共同特征是**不渲染任何东西**：它把「阶段 / 方向 / 连接方式 / 中断原因 / 终态原因」
// 各自的枚举映射成可翻译描述符，把「怎么分桶、怎么算耗时」写成纯函数。因此它可以被列表行、
// 详情侧、以及将来任何需要同一套说法的地方共用，而不必把整个面板 import 进来。
//
// **描述符不在这里展开**：本模块没有 `useLingui()`，映射的值一律是 `msg\`\`` 描述符，
// 由组件 `t(...)` 展开（知识库 Lingui 三条约束之一）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import {
  isActiveSession,
  isRecoverableSession,
  sessionEndedAt,
} from "../_lib/format";
import type { Device, TransferProjection } from "../_lib/view-types";

/**
 * 会话筛选。四档与桌面 `_app/transfer/index.lazy.tsx` **同名，但判据尚未同义**——
 * 桌面的 `active` 是 offered/waiting/active（不含 suspended）、`ended` 还收编了
 * 「不可恢复的中断」，本端两者都只看 `phase === "terminal"` 这条线。于是一条不可恢复
 * 的中断会话在桌面归「已结束」、在这里归「进行中」。这是与「桌面按 startedAt、另两端按
 * updatedAt」同型的漂移，记在 `DESIGN.md` 契约的 open gaps 里，收敛要先裁决哪个对。
 *
 * `recoverable` 单独成一档而不是并进 `active`：它是**要用户做点什么**的那一类
 * （点续传），而 active 是「不用管，它自己在跑」。桌面把这条分得很清楚。
 */
export type SessionFilter = "all" | "active" | "recoverable" | "ended";

export const FILTER_LABEL: Record<SessionFilter, MessageDescriptor> = {
  all: msg`全部`,
  active: msg`进行中`,
  recoverable: msg`可恢复`,
  ended: msg`已结束`,
};

// 只管展示（标签 + 状态点色）。「是否进行中」的判定在 `_lib/format.ts` 的 `isActiveSession`——
// 导航徽标也用它，两处各写一份就会在新增 phase 时对不上。
export const PHASE_META: Record<TransferProjection["phase"], { label: MessageDescriptor; dot: string }> = {
  offered: { label: msg`等待处理`, dot: "bg-amber-500" },
  waiting_accept: { label: msg`等待对方接受`, dot: "bg-amber-500" },
  active: { label: msg`传输中`, dot: "bg-emerald-500" },
  suspended: { label: msg`已中断`, dot: "bg-sky-500" },
  terminal: { label: msg`已结束`, dot: "bg-muted-foreground" },
};

export const DIRECTION_LABEL: Record<TransferProjection["direction"], MessageDescriptor> = {
  send: msg`发送`,
  receive: msg`接收`,
};

export const CONNECTION_LABEL: Record<NonNullable<Device["connection"]>, MessageDescriptor> = {
  lan: msg`局域网`,
  dcutr: msg`打洞直连`,
  relay: msg`中继`,
};

export const SUSPENDED_LABEL: Record<NonNullable<TransferProjection["suspendedReason"]>, MessageDescriptor> = {
  local_paused: msg`本机暂停`,
  remote_paused: msg`对方暂停`,
  interrupted: msg`连接中断`,
  peer_offline: msg`对方离线`,
  app_restarted: msg`应用重启`,
};

export const TERMINAL_LABEL: Record<NonNullable<TransferProjection["terminalReason"]>, MessageDescriptor> = {
  completed: msg`已完成`,
  cancelled: msg`已取消`,
  rejected: msg`已拒绝`,
  fatal_error: msg`失败`,
  // 「没答复」既不是拒绝也不是失败——用户什么都没做，说成任何一种都是替他记了一笔
  // 他没做过的决定。Record 是 exhaustive 的，漏了这条编译期就会红。
  expired: msg`未及时处理`,
};

/**
 * 单条会话是否命中某一档筛选。
 *
 * ## 列表是一条纯时间线（2026-08-12）
 *
 * 此前这里是 `groupSessions`，把结果切成 active / history 两段、「已结束」一段带自己的
 * 小标题。分段读起来像是帮用户分好了类，实际代价是「最近发生了什么」读不出来：一条刚
 * 失败的会话会排在一堆几天前的完成记录之后，因为它在下半段。现在只筛不分，顺序全交给
 * `sortByTimelineDesc`——判据见 `DESIGN.md` 的 **Transfer List Order Contract**。
 *
 * ## 历史不再截断（2026-08-04）
 *
 * 此前已结束会话硬截 8 条，第 9 条起**在 UI 里完全够不着**——只有带 `?session=` 的深链能把
 * 它捞回来，而那条链接的唯一生产者是发送页刚发完那一下。当时的理由是「再多就该去收件箱看
 * 结果」，但收件箱只有**接收**方向，发出去的历史在那里一条都没有。
 *
 * 换成筛选：想只看进行中就点「进行中」，想翻旧账就点「已结束」。列表本身是虚拟滚动之外的
 * 普通列表，几百条会话的 DOM 量级完全撑得住（每行是三行文字 + 一条进度条），真到需要分页
 * 的规模时该做的是分页，而不是悄悄丢掉。
 */
export function matchesSessionFilter(
  projection: TransferProjection,
  filter: SessionFilter,
): boolean {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return isActiveSession(projection);
    case "recoverable":
      return isRecoverableSession(projection);
    case "ended":
      return !isActiveSession(projection);
  }
}

export function connectionByPeer(devices: Device[]) {
  return new Map(devices.map((device) => [device.peerId, device.connection]));
}

/**
 * 下面两个返回**描述符**而非字符串：它们是模块级纯函数，翻译宏在这里只能定义、不能展开
 * （展开要 `useLingui()`，那是组件的事）。调用点拿到描述符自己 `t(...)`。
 *
 * 返回值必须是**稳定引用**：`msg` 宏每次求值都新建一个对象，写在返回位上会让每次调用都
 * 换引用，而这个值是 `TransferActivityItem` 的 prop，那个组件靠 `memo` 让「每秒十余次的
 * 进度事件只重渲染它自己那一行」。所以只从模块级的映射表里取，不现造。
 */
export function connectionLabel(
  projection: TransferProjection,
  connections: Map<string, Device["connection"]>,
): MessageDescriptor | null {
  // 查不到连接方式时返回 `null` 而不是一句「连接类型未知」：那句话在**每一条**历史会话上
  // 都成立（对端早就不在连接表里了），于是列表里每行都挂着同一句不携带任何信息的话，
  // 详情侧的摘要行也被它占掉三分之一。`null` 让调用点整段省掉——同 `TransferMetrics`
  // 「算不出来就不摆这一格」的取舍。
  const connection = connections.get(projection.peerId);
  return connection ? CONNECTION_LABEL[connection] : null;
}

export function phaseLabel(projection: TransferProjection): MessageDescriptor {
  if (projection.phase === "suspended" && projection.suspendedReason) {
    return SUSPENDED_LABEL[projection.suspendedReason];
  }
  if (projection.phase === "terminal" && projection.terminalReason) {
    return TERMINAL_LABEL[projection.terminalReason];
  }
  return PHASE_META[projection.phase].label;
}

export function elapsedSeconds(projection: TransferProjection): number | null {
  const end = sessionEndedAt(projection);
  if (!projection.startedAt || !end || end < projection.startedAt) return null;
  return Math.round((end - projection.startedAt) / 1000);
}

