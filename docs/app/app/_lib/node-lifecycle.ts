// 节点运行时的**启停编排** —— 「让节点跑起来」这件事的唯一定义。
//
// ## 与 `node-runtime.ts` 的分工
//
//   node-runtime.ts    wasm 模块与节点句柄的记忆化（低层，不认识 store、不认识订阅）
//   node-lifecycle.ts  一次启动 = spawn + 回补历史 + 三条订阅 + 引导节点登记 + store 状态，
//                      以及它们各自的回滚
//
// 这一层存在的理由是它有**两个调用方**：应用外壳挂载时（`web-node-bootstrap.tsx`）与用户在
// 节点状态弹窗里手动启停（`node-status-dialog.tsx`）。启动序列若留在组件的 effect 里，
// 弹窗那条路径只能把六个步骤照抄一遍——而漏掉其中任何一步都不会报错，只会让某一类事件
// 从此不到（比如漏了 `startEventConsumption`，节点在跑、传输进度永远是 0）。
//
// ## 订阅句柄是模块级的，不随组件生命周期起落
//
// 运行时是**页面级单例**。StrictMode 的 mount→cleanup→mount 不该把它停掉再起一遍，所以
// 启动是幂等的、组件也不在 cleanup 里停订阅（那正是此前 `cancelled` 标志在兜的事）。
// 真正的停止只有一个来路：用户显式关停。

import { startEventConsumption, stopEventConsumption } from "./event-dispatch";
import { startInfraWatch } from "./infra-watch";
import { closeNode, getNode, spawnNode } from "./node-runtime";
import { infraNodesToReplay, preferencesStore } from "./preferences-store";
import { startStatePoll } from "./state-poll";
import { webNodeActions } from "./store";
import { toWebError } from "./view-types";
import type { WebNode } from "./view-types";

/**
 * 一次运行期间挂上的订阅，停机时逐个调用。
 *
 * **是一个累积的数组而不是一个带具名字段的对象**，因为装配可以中途失败：若写成
 * `subscriptions = { stopPoll: startStatePoll(n), stopInfraWatch: startInfraWatch(n) }`，
 * 后一个调用抛错会让整条赋值语句作废——而前一个已经起了的 `setInterval` 再也收不回来，
 * 且 `subscriptions` 仍是 `null`，下一次启动会若无其事地再起一个。逐个 push 则「已经挂上的」
 * 始终有据可查，失败时照着回滚即可。
 */
let subscriptions: Array<() => void> | null = null;

/**
 * 启停串行化。
 *
 * 两者动的是同一份订阅句柄：交叠执行时，后发的 stop 可能停掉先发的 start 刚挂上的订阅，
 * 留下一个「状态显示运行中、却没有任何事件进来」的节点。UI 上按钮已按状态禁用，这里是
 * 机制兜底——按钮的禁用条件将来会变，这条不会。
 */
let transition: Promise<unknown> = Promise.resolve();

function enqueue<T>(task: () => Promise<T>): Promise<T> {
  // 前一次失败也要接着往下跑，所以 `then` 的两个分支都是 task。
  const run = transition.then(task, task);
  transition = run.catch(() => {});
  return run;
}

/**
 * 回放引导节点清单——浏览器唯一的公网可达入口。
 *
 * **必须在启动时做，不能只留给设置页手点。** 浏览器不 listen 本地 socket：没有
 * circuit 可达地址就既收不到对端拨回，也进不了 DHT（引导节点要先经 identify 被
 * `InfraSupervisor` 认成基础设施节点，才会被 `add_infrastructure_peer` 接进 kad 路由表）。
 *
 * 少了这一步，每次刷新页面后节点都处于网络孤立状态：presence 宣告持续 `QuorumFailed`，
 * 已配对设备恒显示「离线」——而这与「已配对设备刷新后仍在」的产品承诺直接冲突
 * （2026-07-28 实测）。
 *
 * 回放的是**内置清单 − 用户撤销的 + 用户自定义的**（`infraNodesToReplay`），不是某个存下来
 * 的合并快照——理由见 `preferences-store.ts` 的 `InfraNodePreferences`。
 *
 * `infra_ensure` 登记的是**常驻意图**（拨号 / reservation / 断线重建都由 core 的
 * `InfraSupervisor` 收敛），与设置页的手动登记互不冲突。
 *
 * 注意「意图幂等」说的是**状态**不是**回执**：重复登记不会产生第二条关系，但
 * `infra_ensure` 会 reject 一个 `duplicate`（这是刻意的，见 `crates/web/src/node.rs`）。
 * 回放跑在一张空的候选表上，正常路径产不出重复；真撞上（历史偏好里存过一条与内置项
 * 逐字相同的自定义地址）也只是下面那行 console，那条早已登记好了。
 */
function replayInfraNodes(node: WebNode): void {
  for (const addr of infraNodesToReplay(preferencesStore.getState().infraNodes)) {
    try {
      node.infra_ensure(addr);
    } catch (e) {
      // 单条登记失败不该挡住其余功能（局域网直连、已有会话都不依赖它）。走到这里说明
      // 那条地址过不了 core 的校验——内置项是代码里的常量（那就是个 bug），自定义项则是
      // 用户在别的版本里加的、当时的本端还装配着那种 transport。两者都只值一行 console。
      console.error("[web] 引导节点登记失败", addr, e);
    }
  }
}

/**
 * 启动节点运行时。已在跑则直接返回（幂等）。
 *
 * 失败时把错误落进 store（`status: "error"`）**并**抛出：前者服务全局错误条，后者让手动
 * 启动的调用方能就地给出反馈。两者不是重复——弹窗里点「启动节点」失败却只有页面顶部一条
 * 横幅在响应，用户会以为按钮没反应。
 */
export function startNodeRuntime(): Promise<void> {
  return enqueue(async () => {
    if (subscriptions) return;

    webNodeActions.setError(null);
    webNodeActions.setStatus("starting");

    /** 已经挂上的订阅。装配中途失败时照它回滚——见 `subscriptions` 的说明。 */
    const attached: Array<() => void> = [];

    try {
      const node = await spawnNode();

      // 历史回补**先于**事件消费：它是一次性快照，先灌进去就不会与随后的实时事件抢同一个
      // sessionId（`setHistory` 的「已存在的不覆盖」策略正是靠这个顺序才成立）。
      // 读不到历史不该挡住节点可用。
      try {
        webNodeActions.setHistory(await node.transfer_history());
      } catch (e) {
        console.error("[web] transfer_history() 失败，历史与收件箱本次不回补", e);
      }

      startEventConsumption(node); // 源一：transfer 事件流（按实例单点消费）
      attached.push(stopEventConsumption);
      attached.push(startStatePoll(node)); // 源二：pairing 请求 + 已配对设备轮询
      // 源四：基础设施状态流。**必须在运行时层而不是设置页面板里**——它有三个跨路由的
      // 消费者（设置页列清单、设备页读可达地址判断能否生成邀请、常驻徽章算整体健康度）。
      // 见 infra-watch.ts。
      attached.push(startInfraWatch(node));
      replayInfraNodes(node); // 公网可达 + DHT 接线，见函数注释

      subscriptions = attached;
      // 状态最后落：`markRunning` 一置，各页的 `ready` 就全部放行，而放行的前提是上面
      // 那几条订阅都已经在位。
      webNodeActions.markRunning(node.node_id());
    } catch (e) {
      for (const stop of attached) stop();
      subscriptions = null;
      // **节点也要关掉。** spawn 成功而后续装配失败时它还活着，而 `spawnNode()` 是记忆化的
      // ——下一次启动会拿到同一个实例，`events()` 却已经被这次取走过了（每实例只能取一次），
      // 于是那条事件流再也接不上：节点看着在跑，传输进度永远是 0。关掉它，让重试从干净状态开始。
      await closeNode().catch(() => {});
      webNodeActions.setError(toWebError(e));
      throw e;
    }
  });
}

/**
 * 关停节点运行时：停订阅 → 关节点 → 清运行态。
 *
 * **顺序不能反。** 先停订阅：轮询打在一个已关停的节点上会逐 tick 抛错（`state-poll.ts` 里
 * 那三个 try/catch 就是为此），而 `reset()` 若排在关节点之前，界面会先一步清空、真正的
 * 关停却还在途——中间那一段用户看到的是一个「没有任何设备的运行中节点」。
 */
export function stopNodeRuntime(): Promise<void> {
  return enqueue(async () => {
    if (!getNode()) return;

    webNodeActions.setStatus("closing");
    for (const stop of subscriptions ?? []) stop();
    subscriptions = null;

    try {
      await closeNode();
    } finally {
      // 无论关停是否干净，句柄都已经被 `closeNode` 置空、再也用不了了——运行态必须跟着清，
      // 否则界面会留着一份指向不存在节点的设备清单。
      webNodeActions.reset();
    }
  });
}
