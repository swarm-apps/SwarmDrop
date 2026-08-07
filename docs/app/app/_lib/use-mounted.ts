"use client";

// 「这一帧是不是已经在浏览器里了」。
//
// ## 为什么需要它
//
// 静态导出（`output: "export"`）没有服务端，HTML 是**构建时**产出的，那时读不到任何存在
// localStorage 里的东西——主题（next-themes）就是典型：预渲染出来的 HTML 必然是「未选中
// 任何一项」，而首帧客户端已经知道用户选了深色。直接按真实值渲染 → hydration mismatch，
// React 会把整棵子树丢掉重画，控制台一片红。
//
// 标准解法是「首帧先画一个与服务端一致的中性态，挂载后再画真的」。这个 hook 就是那个
// 「挂载后」的信号。
//
// ## 为什么不用 `useSyncExternalStore`
//
// 同目录的 `use-media-query.ts` 解决的是同一类问题，但它用 `useSyncExternalStore` +
// 「服务端快照显式返回窄屏」——那是因为它**有一个外部数据源**（`matchMedia`）要订阅。
// 这里没有外部源，要的只是「渲染过一次了吗」这一个布尔，`useState` + `useEffect` 是它
// 最直白的形式；套 `useSyncExternalStore` 得为它编一个假的 subscribe。
//
// ## 代价说明白
//
// 用它的组件首帧会画中性态（比如主题图标先显示「跟随系统」），下一帧才切成真值。
// 这一帧的闪烁是**故意付的**——比整棵子树报 mismatch 便宜得多。

import { useEffect, useState } from "react";

export function useMounted(): boolean {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  return mounted;
}
