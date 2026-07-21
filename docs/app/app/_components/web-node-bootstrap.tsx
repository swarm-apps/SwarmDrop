"use client";

// 应用外壳的生命周期挂载点（无渲染）：探测 secure-context → spawn 节点 → 接上两条事件源。
// 挂在应用 layout 里，随 /app 存活。
//
// StrictMode（`reactStrictMode: true`）会在开发期 mount→cleanup→mount。本组件**不在 cleanup
// 里 closeNode()**——那会把刚 spawn（或正在 spawn）的页面级单例关掉，第二次 mount 再拿到一个已
// 关闭的实例。节点是页面级单例（spawnNode 记忆化），SPA 内不显式关；标签页关闭由 wasm 的
// FinalizationRegistry 回收。真正需要关停时走设置/退出流程显式调 closeNode()。

import { useEffect } from "react";
import { startEventConsumption } from "../_lib/event-dispatch";
import { spawnNode } from "../_lib/node-runtime";
import { detectSecureContext } from "../_lib/secure-context";
import { startStatePoll } from "../_lib/state-poll";
import { webNodeActions } from "../_lib/store";
import { toWebError } from "../_lib/view-types";

export function WebNodeBootstrap() {
  useEffect(() => {
    let cancelled = false;
    let stopPoll: (() => void) | undefined;

    // 客户端真值校正 SSR 乐观默认；横幅只在此之后才可能出现。
    webNodeActions.setSecure(detectSecureContext());
    webNodeActions.setStatus("starting");

    spawnNode()
      .then((node) => {
        if (cancelled) return;
        webNodeActions.setNodeId(node.node_id());
        webNodeActions.setStatus("running");
        startEventConsumption(node); // 源一：transfer 事件流（单点消费）
        stopPoll = startStatePoll(node); // 源二：pairing 请求 + 已配对设备轮询
      })
      .catch((e) => {
        if (cancelled) return;
        webNodeActions.setError(toWebError(e));
      });

    return () => {
      cancelled = true;
      stopPoll?.();
    };
  }, []);

  return null;
}
