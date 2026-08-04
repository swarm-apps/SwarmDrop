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
import type {
  Device,
  TransferProgressEvent,
  TransferProjection,
} from "../_lib/view-types";


export type TransferFileRow = TransferProgressEvent["files"][number] | TransferProjection["files"][number];

/**
 * 会话筛选。与桌面 `_app/transfer/index.lazy.tsx` 的四档同名同义。
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

/**
 * 详情侧逐文件行的默认展示条数，其余折在「显示全部」后面。
 *
 * 一次传一整个文件夹（几十上百个文件）是常态，而每一行现在都带自己的进度条，进行中的会话
 * 每秒还要重画一遍。全量渲染换不来可读性——超过十几行时用户已经在滚动里找不着自己关心的
 * 那个文件了，真要逐个核对的人才需要展开。
 */
export const FILE_LIST_LIMIT = 12;

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
 * 按筛选分组。入参已排好序——排序只依赖 projections，别和只影响分组的参数挤进同一个 memo，
 * 否则每换一次筛选都要重跑一次全量排序。
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
export function groupSessions(sorted: TransferProjection[], filter: SessionFilter) {
  const active: TransferProjection[] = [];
  const history: TransferProjection[] = [];

  for (const projection of sorted) {
    const live = isActiveSession(projection);
    const matches =
      filter === "all" ||
      (filter === "active" && live) ||
      (filter === "recoverable" && isRecoverableSession(projection)) ||
      (filter === "ended" && projection.phase === "terminal");
    if (!matches) continue;
    if (live) active.push(projection);
    else history.push(projection);
  }

  return { active, history, total: active.length + history.length };
}

export function connectionByPeer(devices: Device[]) {
  return new Map(devices.map((device) => [device.peerId, device.connection]));
}

/**
 * 未知连接方式的兜底标签。**必须是模块级常量**：`msg` 宏每次求值都新建一个对象，写在
 * `connectionLabel` 的返回位上会让它每次调用返回新引用——而这个值是 `TransferActivityItem`
 * 的 prop，那个组件靠 `memo` 让「每秒十余次的进度事件只重渲染它自己那一行」。
 * 新引用会把整张表的 memo 打穿。
 */
export const UNKNOWN_CONNECTION_LABEL = msg`连接类型未知`;

/**
 * 下面两个返回**描述符**而非字符串：它们是模块级纯函数，翻译宏在这里只能定义、不能展开
 * （展开要 `useLingui()`，那是组件的事）。调用点拿到描述符自己 `t(...)`。
 *
 * 两者的返回值都必须是**稳定引用**（见上），所以只从模块级的映射表里取，不现造。
 */
export function connectionLabel(
  projection: TransferProjection,
  connections: Map<string, Device["connection"]>,
): MessageDescriptor {
  const connection = connections.get(projection.peerId);
  return connection ? CONNECTION_LABEL[connection] : UNKNOWN_CONNECTION_LABEL;
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

