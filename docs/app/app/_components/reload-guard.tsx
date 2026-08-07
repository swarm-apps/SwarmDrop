"use client";

// 「发送不跨刷新」的最后一道告知。
//
// 详情面板里那句说明（`transfer-activity-panel.tsx` 的 `isLostOnReload` 提示条）只有在用户
// **正看着那条会话**时才可见，而真正会丢东西的动作——⌘R、关标签页、点回退——可以发生在任何
// 路由下，甚至就发生在用户切到 /app/settings 之后。所以拦截必须挂 layout，与 `WindowDropGuard`
// 同理：它要在任何路由下都生效，而不只是传输页。
//
// 两者是**互补而非重复**：浏览器早已不允许自定义 `beforeunload` 的文案（一律显示厂商的
// 通用措辞），所以这里只能拦住动作、说不出原因；「为什么」由面板那句话讲。反过来，面板那句
// 话拦不住任何操作。缺哪一半都不完整。
//
// **只在真有东西会丢时挂监听**。无条件挂 `beforeunload` 会让每一次正常刷新都弹确认框——
// 接收方向刷新是安全的（半成品在 OPFS 里，续得上），为它拦截纯属打扰，判据 `isLostOnReload`
// 已经把这个不对称表达掉了。

import { useEffect } from "react";
import { isLostOnReload } from "../_lib/format";
import { useWebNode } from "../_lib/store";

export function ReloadGuard() {
  // `Object.values(...).some(...)` 收敛成布尔——派生的中间数组不出 selector，引用每帧都新
  // 也不会触发重渲染（`check:zustand-access` 的 SCALAR_TAIL 放行的正是这个形态）。
  const hasSessionAtRisk = useWebNode((s) => Object.values(s.projections).some(isLostOnReload));

  useEffect(() => {
    if (!hasSessionAtRisk) return;
    const warn = (event: BeforeUnloadEvent) => {
      // 现代浏览器认 preventDefault；`returnValue` 是给老实现的兼容尾巴，两者都给才稳。
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [hasSessionAtRisk]);

  return null;
}
