/**
 * 会话级定时器台账 —— 一个 sessionId 至多一条在途定时器。
 *
 * 活跃传输的界面有两处「到点了要重算一次」的需求，而它们都不是由事件驱动的：
 *
 * - **进度帧变陈旧**：进度事件只从收块路径上发出，没有任何 tick 在推它。对端一安静，
 *   最后那帧就永远躺在 store 里；只在渲染时判时效是不够的 —— 没有新事件就没有重渲染，
 *   界面会一直停在最后一帧画好的样子上。
 * - **发布态延迟揭示**（[`PUBLISH_VISIBLE_AFTER_MS`](./progress)）：常数时间的发布里
 *   `started` 与 `finished` 背靠背到达，急着画就是每收齐一个文件闪一下灰。
 *
 * 两者的清理点完全重合（会话终态 / 记录被删 / 列表重载 / store reset），所以收成同一个
 * 台账原语，而不是在 store 里散着摆两组定时器句柄 —— 漏清一处的症状是「一个早已结束的
 * 会话过几秒突然又长出一条横幅」，而那种 bug 只在特定时序下复现。
 *
 * ## 为什么调度器是参数
 *
 * 本包的 tsconfig 是 `lib: ["ES2022"]` + `types: []`，**`setTimeout` 在这里不存在**
 * （它来自 DOM 或 node 的类型，都是平台依赖，而本包零平台依赖）。所以调度器由调用点注入。
 *
 * ⚠️ **必须包一层箭头函数，不要把 `setTimeout` 当值传进来：**
 *
 * ```ts
 * const timers = createSessionTimers(
 *   (fire, delayMs) => setTimeout(fire, delayMs),
 *   (handle) => clearTimeout(handle),
 * );
 * ```
 *
 * 台账通常是模块级常量，直接传 `createSessionTimers(setTimeout, clearTimeout)` 等于在
 * **模块求值那一刻**把全局函数快照下来，于是：
 *
 * - `vi.useFakeTimers()` 换掉的是 `globalThis.setTimeout`，快照里握着的还是真时钟 ⇒
 *   所有靠 `advanceTimersByTime` 推进的用例一起红，症状是「到点该发生的事成片不发生」，
 *   看起来像业务逻辑坏了。桌面与 Web 在 2026-08-10 各自独立踩了一次。
 * - 浏览器里 `setTimeout` 是 WebIDL 方法，脱离 `window` 单独调用会 `Illegal invocation`
 *   —— 这条纯运行时，测试根本照不到。
 *
 * 收进本包之前它在三端各有一份，且已经开始漂：批量清理只有移动端有、桌面把它内联展开、
 * Web 干脆没有。
 */

export interface SessionTimers {
  /**
   * 给某会话排一条定时器。**同一会话已有在途定时器时先撤掉它** —— 语义是
   * 「从现在起再等 delayMs」，不是「再加一条」。
   */
  schedule(sessionId: string, delayMs: number, fire: () => void): void;
  /** 撤掉某会话的在途定时器。返回**是否真的撤掉了一条**。 */
  cancel(sessionId: string): boolean;
  /** 只保留 `isLive` 认可的会话，其余一律撤掉。列表重载后的批量清理。 */
  retain(isLive: (sessionId: string) => boolean): void;
  /** 撤掉全部（store reset）。 */
  clear(): void;
  /** 在途定时器数 —— 仅供自检 / 测试观察，业务代码不该读它。 */
  readonly size: number;
}

export function createSessionTimers<THandle>(
  setTimer: (fire: () => void, delayMs: number) => THandle,
  clearTimer: (handle: THandle) => void,
): SessionTimers {
  const timers = new Map<string, THandle>();

  function cancel(sessionId: string): boolean {
    const handle = timers.get(sessionId);
    if (handle === undefined) return false;
    clearTimer(handle);
    timers.delete(sessionId);
    return true;
  }

  return {
    schedule(sessionId, delayMs, fire) {
      cancel(sessionId);
      timers.set(
        sessionId,
        setTimer(() => {
          // 先从台账里摘掉再执行：回调里往往会重新排一条（进度陈旧那条就会），
          // 顺序反了会把新排的那条当成旧句柄删掉。
          timers.delete(sessionId);
          fire();
        }, delayMs),
      );
    },
    cancel,
    retain(isLive) {
      for (const sessionId of [...timers.keys()]) {
        if (!isLive(sessionId)) cancel(sessionId);
      }
    },
    clear() {
      for (const handle of timers.values()) clearTimer(handle);
      timers.clear();
    },
    get size() {
      return timers.size;
    },
  };
}
