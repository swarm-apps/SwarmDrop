"use client";

// 应用外壳的生命周期挂载点（无渲染）：探测 secure-context → 读模块级事实 → 起节点运行时。
//
// **只在 layout 这一处挂。** 下放到任何 page 就会变成每路由一份。
//
// 编排本身不在这里——它在 `_lib/node-lifecycle.ts`，因为节点状态弹窗里的「启动节点」要走
// 同一套动作。这个组件只回答「什么时候开始」，不回答「开始要做哪些事」。
//
// **effect 没有 cleanup，这是刻意的。** 运行时是页面级单例：StrictMode 的
// mount→cleanup→mount 不该把它停掉再起一遍（启动是幂等的，重复调用直接返回），
// 关停只有一个来路——用户在弹窗里显式停。

import { useEffect } from "react";
import { getModule } from "../_lib/node-runtime";
import { startNodeRuntime } from "../_lib/node-lifecycle";
import { detectSecureContext } from "../_lib/secure-context";
import { webNodeActions } from "../_lib/store";

/**
 * 灌一次**模块级**事实：设备名（用户设的，可能没有）、回退名（内核 UA 派生的默认值）、
 * 收件箱检索上限。
 *
 * **刻意不等 spawn。** 这三样都不依赖节点是否起得来——节点起不来时设置页照样可达、照样要
 * 显示当前名字。它们只等 `getModule()`（wasm 模块加载），与 `spawnNode()` 共用同一个
 * module promise，所以不会多拉一次 wasm。
 *
 * 同理它们**不属于 `node-lifecycle`**：节点停了再起，这三样不会变，没有重读的必要。
 *
 * 读失败只记日志：设备名是展示信息，拿不到就退化成「空 + 回退名占位」，不该把 layout 打成
 * 错误态挡住收发主路径。
 */
async function loadModuleFacts() {
  try {
    const mod = await getModule();
    // 检索上限先读：它是纯常量 getter，不碰 IndexedDB、不可能失败。排在下面那个 await
    // 之后的话，设备名读失败（该 catch 明说「只记日志」）会连带让它永远是 null，
    // 于是「只显示最近 N 条」那条截断提示整场会话静默消失——而那条提示的全部意义
    // 就是「截断要说出来」。
    webNodeActions.setInboxSearchLimit(mod.inbox_search_limit());
    webNodeActions.setDeviceName((await mod.get_device_name()) ?? null);
    webNodeActions.setDeviceNameFallback(mod.default_device_name());
  } catch (e) {
    console.error("[web] 设备名读取失败，本次以浏览器默认名展示", e);
  }
}

export function WebNodeBootstrap() {
  useEffect(() => {
    // 客户端真值校正 SSR 乐观默认；横幅只在此之后才可能出现。
    webNodeActions.setSecure(detectSecureContext());
    void loadModuleFacts();
    // 启动失败已由 lifecycle 落进 store 的错误域（页面顶部横幅），这里不必再接一次。
    void startNodeRuntime().catch(() => {});
  }, []);

  return null;
}
