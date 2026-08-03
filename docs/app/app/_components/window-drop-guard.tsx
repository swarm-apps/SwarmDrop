"use client";

// 应用区的误投放护栏。
//
// 浏览器对「把文件拖进窗口」的默认行为是**导航到那个文件**——地址栏换成 `file:///…`，
// 当前文档连同它上面跑着的一切一起被销毁。对一个「拖文件进来就发」的应用来说，这个默认值
// 格外致命：用户瞄准投放区、手抖偏了两厘米，代价不是「这次没加上文件」，而是**正在跑的
// P2P 节点被销毁**——连接断开、进行中的传输中止，回来还要重新 spawn 一遍。
//
// 所以在应用区窗口级把 `dragover` / `drop` 的默认行为一律拦掉。真正的投放目标（发送卡片）
// 自己 `preventDefault` 并读 `dataTransfer.files`，React 的事件委托跑在 document 上、
// 早于这里的 window 监听，功能不受影响。
//
// **只挂在 `/app` 的 layout**，文档站其它页面不受影响（那里没有什么怕被拖没的运行时）。

import { useEffect } from "react";

export function WindowDropGuard() {
  useEffect(() => {
    // dragover 也必须拦：只拦 drop 是**无效**的——不 preventDefault dragover，浏览器压根
    // 不把窗口当成有效投放目标，drop 事件不会派发，默认导航照常发生。
    const swallow = (event: DragEvent) => {
      // 真正的投放目标（发送卡片）已经处理过这次事件——它自己 `preventDefault` 并把
      // `dropEffect` 设成了 copy。这里不能再插手：window 监听跑在冒泡的最末端，覆盖
      // `dropEffect` 会把投放区的「可以放」光标改回禁止符。
      if (event.defaultPrevented) return;
      event.preventDefault();
      // 拦下之后要**说清楚这里放不了**。只 preventDefault 而不改 dropEffect，整个窗口都会
      // 显示「可以放」的光标，放下去却什么也不发生——比原来的导航走人更难理解。
      if (event.type === "dragover" && event.dataTransfer) {
        event.dataTransfer.dropEffect = "none";
      }
    };
    window.addEventListener("dragover", swallow);
    window.addEventListener("drop", swallow);
    return () => {
      window.removeEventListener("dragover", swallow);
      window.removeEventListener("drop", swallow);
    };
  }, []);

  return null;
}
