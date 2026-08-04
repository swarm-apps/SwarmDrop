"use client";

// 应用外壳的生命周期挂载点（无渲染）：探测 secure-context → 读设备名 → spawn 节点 →
// 接上两条事件源。挂在应用 layout 里，随 /app 存活。
//
// **只在 layout 这一处做。** 下放到任何 page 就会变成每路由一份：同一事件被消费多次、
// 轮询与 relay 登记也各起一份。
//
// StrictMode（`reactStrictMode: true`）会在开发期 mount→cleanup→mount。本组件**不在 cleanup
// 里 closeNode()**——那会把刚 spawn（或正在 spawn）的页面级单例关掉，第二次 mount 再拿到一个已
// 关闭的实例。节点是页面级单例（spawnNode 记忆化），SPA 内不显式关；标签页关闭由 wasm 的
// FinalizationRegistry 回收。真正需要关停时走设置/退出流程显式调 closeNode()。

import { useEffect } from "react";
import { startEventConsumption } from "../_lib/event-dispatch";
import { getModule, getNode, spawnNode } from "../_lib/node-runtime";
import { WEB_RELAY_HELPERS } from "../_lib/relay-helpers";
import { detectSecureContext } from "../_lib/secure-context";
import { startRelayWatch } from "../_lib/relay-watch";
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
      node.relays_ensure(addr);
    } catch (e) {
      // 单个 helper 登记失败不该挡住其余功能（局域网直连、已有会话都不依赖它）。
      // 后续的连接/失败/重试状态由「连接」区订阅 `relays_changed()` 呈现，这里只管登记。
      console.error("[web] relay helper 登记失败", addr, e);
    }
  }
}

/**
 * 灌一次设备名：当前值（用户设的，可能没有）+ 回退名（内核 UA 派生的默认值）。
 *
 * **刻意不挂在 spawn 成功之后。** 改名能力不依赖节点状态——节点起不来时设置页照样可达、
 * 照样要显示当前名字，所以它只等 `getModule()`（wasm 模块加载）而不等 `spawnNode()`。
 * 两者共用同一个 module promise，故这里不会多拉一次 wasm。
 *
 * 读失败只记日志：设备名是展示信息，拿不到就退化成「空 + 回退名占位」，不该把 layout 打成
 * 错误态挡住收发主路径。
 */
async function loadDeviceName(isCancelled: () => boolean) {
  try {
    const mod = await getModule();
    // 检索上限先读：它是纯常量 getter，不碰 IndexedDB、不可能失败。排在下面那个 await
    // 之后的话，设备名读失败（该 catch 明说「只记日志」）会连带让它永远是 null，
    // 于是「只显示最近 N 条」那条截断提示整场会话静默消失——而那条提示的全部意义
    // 就是「截断要说出来」。
    webNodeActions.setInboxSearchLimit(mod.inbox_search_limit());
    const current = await mod.get_device_name();
    if (isCancelled()) return;
    webNodeActions.setDeviceName(current ?? null);
    webNodeActions.setDeviceNameFallback(mod.default_device_name());
  } catch (e) {
    console.error("[web] 设备名读取失败，本次以浏览器默认名展示", e);
  }
}

export function WebNodeBootstrap() {
  useEffect(() => {
    let cancelled = false;
    let stopPoll: (() => void) | undefined;
    let stopRelayWatch: (() => void) | undefined;

    // 客户端真值校正 SSR 乐观默认；横幅只在此之后才可能出现。
    webNodeActions.setSecure(detectSecureContext());
    webNodeActions.setStatus("starting");

    void loadDeviceName(() => cancelled);

    spawnNode()
      .then(async (node) => {
        if (cancelled) return;
        webNodeActions.setNodeId(node.node_id());
        webNodeActions.setStatus("running");
        // 源三先于源一：历史回补是一次性快照，先灌进去就不会与随后的实时事件抢同一个
        // sessionId（#81 刷新后收件箱与传输活动仍在）。读不到历史不该挡住节点可用。
        //
        // `transfer_history()` 是 async（端口方法本就是 async），**这个 await 不能挪到
        // `startEventConsumption` 之后**——顺序反了实时事件可能先落域，而 `setHistory`
        // 的「已存在的不覆盖」策略正是靠这个顺序才对。
        try {
          webNodeActions.setHistory(await node.transfer_history());
        } catch (e) {
          console.error("[web] transfer_history() 失败，历史与收件箱本次不回补", e);
        }
        if (cancelled) return;
        startEventConsumption(node); // 源一：transfer 事件流（单点消费）
        stopPoll = startStatePoll(node); // 源二：pairing 请求 + 已配对设备轮询
        ensureConfiguredRelays(node); // 公网可达 + DHT 接线，见函数注释
        // 源四：relay 状态流。**必须在这里而不是「连接」面板里**——它有两个跨路由的
        // 消费者（设置页列清单、设备页读可达地址判断能否生成邀请），绑在某一页上会让
        // 直接进设备页的用户看到一个永远禁用的「生成邀请」。见 relay-watch.ts。
        stopRelayWatch = startRelayWatch(node);
      })
      .catch((e) => {
        if (cancelled) return;
        webNodeActions.setError(toWebError(e));
      });

    return () => {
      cancelled = true;
      stopPoll?.();
      stopRelayWatch?.();
    };
  }, []);

  return null;
}
