/**
 * 深链送来的待处理配对邀请 —— 一个跨「URL 拦截」与「React 树」的单槽交接点。
 *
 * 为什么需要它：入站 URL 由 expo-router 的 `+native-intent.tsx` 接（那是本 App **唯一**的
 * URL 分发口，见那个文件的注释）。它跑在 React 之前，不能 push 路由、也不能碰原生模块，
 * 所以只能把邀请文本放下、交给树里的 handler 取。
 *
 * **刻意零依赖**（不 import zustand、不 import core）：`+native-intent.tsx` 在 App 冷启动
 * 最早期就会 import 本模块，此刻原生桥可能还没起来 —— 依赖越少越不会在那一刻炸。
 *
 * 与桌面 `external_open.rs` 的缓冲同构，两条设计也一致：
 * - **只留最后一条**：同时收到两条邀请是异常（一次只配一台设备），攒成数组只会让 UI
 *   面对一个没有正确答案的选择。
 * - **取走即清空**：保证同一条不被处理两次（冷启动取一次 + 热启动订阅各一次）。
 */

let pending: string | null = null;
const listeners = new Set<() => void>();

/** 放下一条邀请（覆盖旧的）并通知订阅者。 */
export function stashPendingInvite(invite: string): void {
  pending = invite;
  for (const l of listeners) l();
}

/** 取走待处理邀请（取走即清空）。 */
export function takePendingInvite(): string | null {
  const v = pending;
  pending = null;
  return v;
}

/**
 * 订阅「有新邀请放下」。返回退订函数。
 *
 * 热启动（App 已在前台/后台）时 `+native-intent.tsx` 会再次 stash，而树里的 handler
 * 早已 mount、不会重新执行挂载逻辑 —— 所以光有 `take` 不够，必须能被通知。
 */
export function subscribePendingInvite(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
