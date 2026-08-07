"use client";

// 设备页页头右侧的三格概览。对应桌面 `_app/devices/index.lazy.tsx` 的 `HomeOverview`
// 统计区——两份实现、同一套信息。
//
// ## 三个数为什么不是桌面端那三个
//
// 桌面端是「附近 / 已配对 / 传输中」。**「附近」在浏览器里不成立**：它靠 mDNS 局域网发现，
// 而浏览器没有这个能力（`devices/page.tsx` 的「刻意不引入的一块」）。摆一个恒为 0 的格子
// 比不摆更糟——它会让人以为附近真的没有设备，而不是这一端压根不找。
//
// 换上的是**在线**：已配对设备里此刻能收文件的有几台。它是这一页最该被一眼看到的数
// （离线设备不提供发送，见 Send Entry Contract），而下方 `SectionHeader` 的计数只给总数。
// 「已配对」与那个计数确实重复一次，留着是因为三个格子要能互相参照——只有「在线 2」时，
// 用户没法判断是「2/2 全在线」还是「2/9 大半离线」。
//
// ## 为什么不做成桌面端那样一整块横幅
//
// 桌面端 `HomeOverview` 自带 eyebrow + 大标题 + 一句定位，而 Web 端 `PageHeader` 已经渲染了
// 同样的标题与描述。再加一整块就是同一句话说两遍，还多占一屏高度。桌面端那块的实际形态
// 本来就是「标题在左、统计在右」，所以这里补的是**它缺的那一半**，不是它的复制品。

import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { useMemo } from "react";
import { isActiveSession } from "../_lib/format";
import { useWebNode } from "../_lib/store";

/**
 * 三个格子的定义。标签存 `msg` 描述符、由组件 `t(...)` 展开——`_lib/` 之外也守这条：
 * 模板串一旦离开组件的词法作用域，Lingui 宏就提取不到它（见 theme-and-styling.md）。
 */
const STATS = [
  { key: "online", label: msg`在线` },
  { key: "paired", label: msg`已配对` },
  { key: "active", label: msg`传输中` },
] as const;

export function DeviceStats() {
  const { t } = useLingui();
  // selector 只返回 store 内的稳定引用，派生放 `useMemo`——`pnpm check:zustand-access` 规则 B。
  // 这里**只订阅 `pairedDevices` 一次**：既然已经拿到数组，再单独订一个 `.length` 不会少一次
  // 重渲染（同一份引用变了两个 selector 都要重算），只是多一处要跟着维护的读取点。
  const devices = useWebNode((s) => s.pairedDevices);
  const projections = useWebNode((s) => s.projections);

  const onlineCount = useMemo(
    () => devices.filter((d) => d.status === "online").length,
    [devices],
  );
  const activeCount = useMemo(
    () => Object.values(projections).filter(isActiveSession).length,
    [projections],
  );

  const value = { online: onlineCount, paired: devices.length, active: activeCount };

  return (
    // `shrink-0`：窄屏下它整块换行到描述下方（页头 `flex-wrap`），不被压扁成三条竖排。
    <dl className="flex shrink-0 gap-[var(--space-in-group)]">
      {STATS.map((stat) => (
        <div
          key={stat.key}
          // 与桌面端 `OverviewStat` 同形：数字在上、标签在下、居中。玻璃留给结构容器，
          // 这里用 `glass-control`（同 `SectionHeader` 的图标片）——它装着内容，不是控件。
          //
          // **`flex-col-reverse` 不是排版偏好，是为了让 DOM 顺序合法**：`<dl>` 规定
          // `<dt>` 必须排在它的 `<dd>` 之前，而视觉上要数字在上。反着写（dd 在前）是无效
          // HTML，读屏对配对关系的解析也会失准。用 reverse 把两者都满足。
          className="glass-control flex min-w-[68px] flex-col-reverse items-center gap-0.5 rounded-[var(--radius-panel-sm)] px-3 py-2"
        >
          <dt className="text-[11px] text-muted-foreground">{t(stat.label)}</dt>
          {/* `tabular-nums`：数字每秒都可能变（在线数随 presence 轮询、传输数随事件），
              等宽数字位让格子宽度不随内容跳动。 */}
          <dd className="font-mono text-lg font-semibold tabular-nums text-foreground">
            {value[stat.key]}
          </dd>
        </div>
      ))}
    </dl>
  );
}
