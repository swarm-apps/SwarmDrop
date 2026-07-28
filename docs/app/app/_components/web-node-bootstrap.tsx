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
import { getNode, spawnNode } from "../_lib/node-runtime";
import { WEB_RELAY_HELPERS } from "../_lib/relay-helpers";
import { detectSecureContext } from "../_lib/secure-context";
import { startStatePoll } from "../_lib/state-poll";
import { webNodeActions } from "../_lib/store";
import { toWebError } from "../_lib/view-types";

/**
 * 登记配置好的 relay helper——浏览器唯一的公网可达入口。
 *
 * **必须在启动时做，不能只留给「连接」区手点。** 浏览器不 listen 本地 socket：没有
 * circuit 可达地址就既收不到对端拨回，也进不了 DHT（bootstrap 要先经 identify 被
 * `InfraSupervisor` 认成基础设施节点，才会被 `add_infrastructure_peer` 接进 kad 路由表）。
 *
 * 少了这一步，每次刷新页面后节点都处于网络孤立状态：presence 宣告持续
 * `QuorumFailed`，已配对设备恒显示「离线」——而这与「已配对设备刷新后仍在」的产品
 * 承诺直接冲突（2026-07-28 实测）。
 *
 * `relays_ensure` 是**幂等的常驻意图**（拨号 / reservation / 断线重建都由 core 的
 * `InfraSupervisor` 收敛），与「连接」区的手动登记互不冲突，用户仍可另加 helper 或撤销。
 */
function ensureConfiguredRelays(node: ReturnType<typeof getNode>) {
  if (!node) return;
  for (const addr of WEB_RELAY_HELPERS) {
    try {
      const helperId = node.relays_ensure(addr);
      // 首次 active 时回填 circuit 地址给「连接」区展示。单个 helper 起不来不该挡住
      // 其余功能（局域网直连、已有会话都不依赖它），故只记日志。
      void node
        .relays_until_active(helperId)
        .then((circuit) => webNodeActions.setReservation(circuit))
        .catch((e) => console.error("[web] relay helper 未能建立可达", addr, e));
    } catch (e) {
      console.error("[web] relay helper 登记失败", addr, e);
    }
  }
}

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
        // 源三先于源一：历史回补是同步快照，先灌进去就不会与随后的实时事件抢同一个
        // sessionId（#81 刷新后收件箱与传输活动仍在）。读不到历史不该挡住节点可用。
        try {
          webNodeActions.setHistory(node.transfer_history());
        } catch (e) {
          console.error("[web] transfer_history() 失败，历史与收件箱本次不回补", e);
        }
        startEventConsumption(node); // 源一：transfer 事件流（单点消费）
        stopPoll = startStatePoll(node); // 源二：pairing 请求 + 已配对设备轮询
        ensureConfiguredRelays(node); // 公网可达 + DHT 接线，见函数注释
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
