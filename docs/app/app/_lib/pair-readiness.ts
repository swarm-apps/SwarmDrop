// 消费邀请之前的**网络就绪等待**。
//
// ## 为什么这条路径需要一道网络门，而它一直没有
//
// 生成邀请那侧早就有（没有 circuit 可达地址就没有可拨地址，按钮直接禁用 +
// 一整段解释文案）。消费这侧却只判了 `status === "running"` —— 而 running 只说明 wasm
// 节点 spawn 完成、订阅装好了；引导节点此刻才刚被 `replayInfraNodes` 登记成**意图**，
// core 的 InfraSupervisor 最迟 1s 后才发出第一轮拨号，拨通 + reservation 还要几秒。
//
// 这个窗口不是理论上的，它恰好就是从 `/p/` 落地页进来的人所在的位置：确认卡由
// `pairing-panel` 的补偿 effect 在 `ready` 翻真那一刻解码出来，「打开链接 → 立刻点确认」
// 必然落在窗口里，于是**必失败**。
//
// ## 为什么等的是 reservation，而不是「连上了」
//
// 跨网时邀请里唯一用得上的是对方的 circuit 地址，而 circuit 地址的**外层**是对方连中继
// 用的那种传输（桌面是 TCP，见 `crates/net/src/transport.rs` 里 `supported_transports`
// 的说明）。浏览器拨不动 TCP —— 它只有在**自己已经连上同一台中继**时，libp2p 才会复用
// 那条现成连接做 HOP。所以「本机与中继的关系」才是这里真正的前提。
//
// reservation（`selectReservation`）是这层关系可观测的最强形态：它蕴含「已连上」，
// 并且额外保证配对成功之后对方拨得回来。用更弱的 `connected` 只能省下一两秒，
// 却要多解释一次两者的差别。
//
// ## 它只推迟，不否决
//
// 等待超时**不阻止握手**。同一局域网内邀请里带着对方的 webrtc-direct 直连地址，
// 那条路径与中继毫无关系；把等待做成前置条件，等于让一个纯优化的时机调整反过来
// 掐死原本能成的配对。超时后照常发起，失败仍由 `connect_invite` 自己报错。

import { selectReservation, webNodeStore } from "./store";

/**
 * 等中继的耐心上限。
 *
 * 收敛环最迟 1s 起第一轮拨号，webrtc-direct 握手 + reservation 顺利时 2~5s，
 * 移动网络下十几秒也见过。20s 之后再等下去，用户已经在怀疑按钮坏了 —— 而这里超时
 * 不是失败，只是「不等了，直接试」，所以这个数偏保守没有代价。
 */
export const PAIRING_READINESS_TIMEOUT_MS = 20_000;

/**
 * 等到本机能拨中继为止，返回「是否等到了」。
 *
 * 已经就绪时**同步**返回一个已兑现的 Promise —— 最常见的路径（节点早就跑着，用户在设备页
 * 慢慢粘贴邀请）因此不多等哪怕一帧。
 *
 * 三种提前收场都以 `false` 兑现，而不是抛错：
 *
 * - **超时** —— 见文件头，等待只推迟不否决；
 * - **节点被停掉**（用户在节点状态弹窗里按了停止）—— 那不是「还没好」而是「不会好了」，
 *   继续等到超时只是让人多盯二十秒；
 * - **`signal` 被 abort** —— 调用方自己要走。
 *
 * ⚠️ **`false` 不区分这三者，调用方要自己看 `signal.aborted`**：前两种是「别等了，直接
 * 试试看」，第三种是「别试了」。混为一谈的后果是用户点了取消、邀请却照样被消费掉 ——
 * 而邀请是一次性的，那一下不可逆。
 */
export function waitForPairingReadiness(
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<boolean> {
  const state = webNodeStore.getState();
  if (selectReservation(state) !== null) return Promise.resolve(true);
  if (signal?.aborted) return Promise.resolve(false);
  // **进场时就得判一次 `status`。** 下面那条订阅只在**变化**时触发，而节点若已经停了
  // （或启动失败），可能再也不会有下一次 setState —— 于是这里会一动不动地坐满超时，
  // 正好违反上面刚承诺的「不会好了就别等」。
  if (state.status !== "running") return Promise.resolve(false);

  return new Promise((resolve) => {
    let unsubscribe: (() => void) | null = null;
    const onAbort = () => settle(false);
    const settle = (ready: boolean) => {
      clearTimeout(timer);
      unsubscribe?.();
      signal?.removeEventListener("abort", onAbort);
      resolve(ready);
    };
    const timer = setTimeout(() => settle(false), timeoutMs);
    signal?.addEventListener("abort", onAbort);
    unsubscribe = webNodeStore.subscribe((state) => {
      if (selectReservation(state) !== null) settle(true);
      else if (state.status !== "running") settle(false);
    });
  });
}
