"use client";

// 设备网格 + 配对面板的组合层。
//
// ## 为什么需要这一层
//
// `devices/page.tsx` 导出 `metadata`，所以它是 server component，拿不了 `useState`。而
// 「添加设备」的入口在网格里、落点在配对面板里，两者需要共享一点状态。这一层的全部职责
// 就是那点状态——它刻意不做别的，不要往里搬业务。
//
// ## 用命令式句柄，不用受控 prop
//
// 「用户刚按了添加设备」是一次**事件**，不是一个状态。做成 `open: boolean` 要调用方复刻
// 配对面板内部那套三层开合规则；做成「请求计数 + useEffect」是拿 effect 当事件处理器，
// 得凭空编一个只增不减的数字。`ref.current?.open()` 直说那件事。
//
// ## 为什么不把配对做成弹窗或抽屉
//
// 配对时用户要在两个界面之间来回：把自己生成的邀请发给对方，再把对方发来的邀请粘回来。
// 遮罩层会在这个来回中反复挡住已配对设备列表——而那正是他想看到变化的地方（「加进来了吗」）。
// 就地展开则是列表和表单同屏，配对成功时新设备直接出现在上面那格里。
//
// 这也是 DESIGN.md 那句「其余 UI 保持安静，好让环境层承载个性」的一个实例：
// 少一层浮层，就少一次和背后那片极光抢注意力。

import { useRef } from "react";
import { DeviceGrid } from "./device-grid";
import { PairingPanel, type PairingPanelHandle } from "./pairing-panel";

export function DevicesSection() {
  const pairing = useRef<PairingPanelHandle>(null);

  return (
    <>
      <DeviceGrid onAddDevice={() => pairing.current?.open()} />
      <PairingPanel ref={pairing} />
    </>
  );
}
