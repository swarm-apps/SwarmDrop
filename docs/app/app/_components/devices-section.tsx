"use client";

// 设备页正文的布局层：设备网格 + 活跃传输 + 配对面板。
//
// ## 为什么需要这一层
//
// `devices/page.tsx` 导出 `metadata`，所以它是 server component，拿不了 `useState`。而
// 「添加设备」的入口在网格里、落点在配对面板里，两者需要共享一点状态；分栏之后还多了一件
// 只有客户端知道的事——**当前是不是宽屏**（配对面板的默认展开态跟着它翻转，见下）。
//
// 这一层只做「状态 + 版式」，不做业务。
//
// ## 用命令式句柄，不用受控 prop
//
// 「用户刚按了添加设备」是一次**事件**，不是一个状态。做成 `open: boolean` 要调用方复刻
// 配对面板内部那套三层开合规则；做成「请求计数 + useEffect」是拿 effect 当事件处理器，
// 得凭空编一个只增不减的数字。`ref.current?.open()` 直说那件事。
//
// ## 版式：≥1280px 时「主栏 + 配对侧栏」，窄屏竖排
//
// 此前三块一律竖排全宽，于是**配对面板里那些窄内容被拉到 1150px**：一个单行输入框横跨
// 整屏，「仅同一网络内可用」的标签在最左、它的开关在最右，视线要横扫一整屏才能把一对
// 控件对上。而设备网格恰恰相反——它是唯一真正吃得下宽度的一块（`auto-fill` 每多 288px
// 就多一列）。把配对收进一条 360px 的侧栏，两边同时变好。
//
// **仍然不是弹窗、不是抽屉、不是子路由**（这条没变）：配对时用户要在两个界面之间来回，
// 把自己生成的邀请发给对方、再把对方发来的邀请粘回来。遮罩会在这个来回中反复挡住已配对
// 设备列表——而那正是他想看到变化的地方（「加进来了吗」）；子路由是同一个问题的更彻底
// 版本，整个列表都不在了。分栏比原来的就地展开更进一步：两块**同时**可见，配对成功时
// 新设备直接出现在旁边那栏，不必先把面板收起来才看得见。
//
// 这也是 DESIGN.md 那句「其余 UI 保持安静，好让环境层承载个性」的一个实例：
// 少一层浮层，就少一次和背后那片极光抢注意力。
//
// ## 断点为什么是 1280 而不是全局那个 920
//
// 920 量的是主从（两栏都是内容），这里是「主内容 + 一栏辅助工具」；更要紧的是 Web 应用区
// 的导航侧栏本身占位（≥1024 那档 224px），同一个视口宽度在两端剩下的内容宽并不一样。
// 完整推导在 `_lib/use-media-query.ts` 的 `DEVICES_SPLIT_QUERY`。
//
// ⚠️ **CSS 的 `xl:` 与 JS 的 `DEVICES_SPLIT_QUERY` 必须是同一个数**，两者一起翻转：
// 栅格变两栏的同一刻，配对面板要从「默认收起」变成「默认展开」。差一档就会出现
// 「侧栏已经分出来了，里面却只有一行收起的标题」——一条 360px 宽的空栏。

import { useRef } from "react";
import { ActiveTransfersSection } from "./active-transfers-section";
import { DeviceGrid } from "./device-grid";
import { PairingPanel, type PairingPanelHandle } from "./pairing-panel";
import { useIsDevicesSplit } from "../_lib/use-media-query";

export function DevicesSection() {
  const pairing = useRef<PairingPanelHandle>(null);
  const split = useIsDevicesSplit();

  return (
    <div className="grid gap-[var(--space-section)] xl:grid-cols-[minmax(0,1fr)_360px] xl:items-start">
      {/* 主栏。`min-w-0` 是必须的：网格子项的默认 `min-width:auto` 会让长设备名把整栏撑宽，
          于是侧栏被挤出视口——这类溢出在有内容之前看不出来。 */}
      <div className="flex min-w-0 flex-col gap-[var(--space-section)]">
        <DeviceGrid onAddDevice={() => pairing.current?.open()} />
        <ActiveTransfersSection />
      </div>

      {/* 配对栏。`xl:items-start`（在父级）让它按内容高度收，不被拉到与主栏等高——
          主栏有十几台设备时，一条被拉长的空玻璃比不齐更难看（DESIGN.md 的
          Layout Density Contract：「Copy the rule, not the conclusion」）。 */}
      <PairingPanel ref={pairing} defaultOpen={split} />
    </div>
  );
}
