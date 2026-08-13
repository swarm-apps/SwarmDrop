// Web 应用区的状态层：镜像桌面 `src/stores/network-store` 的思路，但事件源是「三轨」的——
//   源一：transfer 域事件走 `events()` 的 ReadableStream（单点消费，见 event-dispatch.ts）；
//   源二：pairing 入站请求、挂起 offer + 已配对设备走同步 getter 轮询（见 state-poll.ts）；
//   源三：`transfer_history()` 在 spawn 后一次性回补 IndexedDB 里的历史投影（活动视图
//         跨刷新，见 web-node-bootstrap.tsx）；
//   源四：`inbox_items()` 按需拉取收件箱真表（**不是** projections 的派生物，见
//         `inboxItems` 域的注释；拉取时机由 InboxPanel 自己掌握）。
// 四者都汇入本 store。actions 独立于 state（不塞进 state 对象），保证 selector 快照稳定。

import {
  createSessionTimers,
  PROGRESS_STALE_MS,
  PUBLISH_VISIBLE_AFTER_MS,
} from "@swarmdrop/shared-view";
import { useStore } from "zustand";
import { createStore } from "zustand/vanilla";
import { isActiveSession } from "./format";
import type { SecureContextInfo } from "./secure-context";
import type {
  InfraLink,
  ConnectionJson,
  Device,
  FilePublishEvent,
  InboxItemDetail,
  OfferJson,
  PendingPairingJson,
  PrepareProgressEvent,
  TransferOfferEvent,
  TransferProgressEvent,
  TransferProjection,
  TransferRejectedEvent,
  WebError,
  WebTransferEvent,
} from "./view-types";

/** 节点前端生命周期（对齐桌面 NodeStatus，另加 closing——由 closeNode 触发）。 */
export type NodeStatus = "idle" | "starting" | "running" | "closing" | "error";

/** 事件流留痕上限——仅供 dev 面板可见 + 证明「事件流接上、零丢弃」，非主 UI 状态源。 */
const EVENT_LOG_CAP = 50;

/**
 * 身份持久化位置。当前基座只支持主线程 Window 运行（`spawnNode` 未提供 Worker 路径，
 * `WebNode::spawn` 也要求主线程——见 node-runtime.ts 注释），故这是编译期常量，不是探测值；
 * Worker 模式落地时（对应 OPFS）把这里换成按运行环境派生即可。
 *
 * **它是 API 名，所以是裸字符串而不是 `msg`**：`localStorage` 在任何语言里都写作
 * `localStorage`，进 catalog 只会得到三份一模一样的条目。此前它是
 * `msg\`localStorage（Window 主线程）\`` —— 那个括号里的中文限定语与 API 名黏在同一个
 * 串里，界面上整串套 `font-mono`，CJK 落回非等宽字体、字距还被撑开，跟紧随其后的正文
 * 明显不是一套字（Mono Truth Rule 只管字面值，不管散文）。限定语现在只留在这条注释里：
 * 今天没有 Worker 模式，「主线程」对用户不构成可据以行动的信息。
 */
export const IDENTITY_LOCATION = "localStorage";

/**
 * 接收面板所需的稳定 offer 视图。
 *
 * 事件流的 `TransferOfferEvent` 比 `pending_offers()` 快照多出来源与策略字段；接收 UI
 * 当前只消费两者共有的文件/对端字段，故在边界归一，避免把「首次事件」与「启动回补」分成
 * 两套 state 与渲染路径。
 */
export interface IncomingOffer {
  sessionId: string;
  peerId: string;
  deviceName: string;
  totalSize: number;
  files: Array<{
    fileId: number;
    name: string;
    relativePath: string;
    size: number;
  }>;
}

export interface WebNodeState {
  // —— node 域 ——
  status: NodeStatus;
  /** base58 身份；刷新后不变（内核 identity::load_or_create 持久化到 localStorage）。 */
  nodeId: string | null;
  /**
   * 本次启动的时刻（Unix 毫秒）；非 running 时为 null。
   *
   * 只服务节点状态弹窗那一行「已运行多久」。**不从 `nodeId` 或事件推**——身份跨重启不变，
   * 而这里要的恰恰是「这一次跑了多久」。
   */
  startedAt: number | null;
  error: WebError | null;
  /** secure-context 探测结果；null = 尚未探测（SSR 快照）。 */
  secure: SecureContextInfo | null;
  /**
   * 用户设定的设备名（内核 `get_device_name()`）；null = 未设，对外展示回落到
   * `deviceNameFallback`。
   *
   * 它**不属于节点域的运行态**——改名能力不依赖节点是否起得来，故灌值时机与 spawn 解耦
   * （见 web-node-bootstrap.tsx）。
   *
   * 写入点只有两个，都在改名成功之后：启动时的一次回灌，以及 `renameDevice()` 的返回值
   * （node-panel.tsx）。**没有订阅式的 `DeviceRenamed` 通道，也不需要**——core 那条事件在
   * Web 侧只落日志（device 域事件整体未 surface 到 JS），而浏览器一个标签页一个节点、
   * 改名的唯一发起点就是本页 UI，那次调用本身就是最准的信号。
   *
   * 节点在跑时改名**即时生效**：`WebNode::rename_device` 落盘、更新本机 OsInfo 并经
   * identify 把新 agent_version 逐连接下发给已连接的对端，不必等下一次 spawn。
   */
  deviceName: string | null;
  /**
   * 未设名时对外展示的默认值（内核 `default_device_name()`，UA 派生的浏览器名）。
   * null = 尚未从 wasm 模块读到。**不在 TS 里另解析一份 UA**——两份判定表迟早漂。
   */
  deviceNameFallback: string | null;

  // —— transfer 域（以 projection 为主状态源）——
  /**
   * 传输投影：前端主状态源（内核逐步以 transferProjection 事件替代分散的终态事件）。
   * 传输活动视图由它派生，故 #81 的跨刷新持久化只需在启动时把 `transfer_history()`
   * 回补进来（见 `setHistory`）。
   *
   * **收件箱不再由它派生**——那是另一张表，见 `inboxItems`。
   */
  projections: Record<string, TransferProjection>;
  /** 挂起入站 offer（按 sessionId）。#79：以非阻断通知/收件箱形式浮现，接受/拒绝后从此域移除。 */
  offers: Record<string, IncomingOffer>;
  /**
   * 对方拒绝 Offer 的原因（按 sessionId，发送侧用）。#79 验收标准之一：未配对对端硬拒
   * `NotPaired` 时前端要给清晰提示而非静默失败——SendPanel 据此渲染，而非让「已发出」
   * 成功态永久悬空。终态事件，不随其他域裁剪。
   */
  rejections: Record<string, TransferRejectedEvent>;
  /**
   * 当前活跃的发送准备批次（一遍流式读产出 checksum + 验签树），由**首条事件自我认领**。
   *
   * **不能挂进 `progress`**：准备阶段还没有 `sessionId`，会话记录要等准备跑完、发出
   * Offer 时才创建。`send_files()` 内部生成的 `preparedId` 也不回传给调用方，而事件先于
   * 它 resolve 到达，所以调用方无从「提前」认领。
   *
   * 此前这里是 `latestPrepareProgress`（旁路快照，永不清空）加一张 `prepares` 表（形状
   * 正确却零读者）。收成一个字段：那张表唯一的读者本就是「活跃的那一条」。
   */
  activePrepare: PrepareProgressEvent | null;
  /**
   * 最近一次被 `clearPrepare` 收掉的批次 id。
   *
   * 事件从 wasm 经事件泵进来，与 `send_files()` 的 promise resolve 是两条路，顺序无保证：
   * 收尾那条 100% 事件可能在清理之后才到，于是活跃位被一个已结束的批次重新占住，进度行
   * 永久停在满格。
   */
  clearedPreparedId: string | null;
  /**
   * 实时进度：speed / eta / 单文件粒度。**TransferProjection 从不携带这些字段**，故单独建域
   * （projection 只表达 phase + 累计字节，不会取代 progress）。供 #80 传输视图直接消费。
   */
  progress: Record<string, TransferProgressEvent>;
  /**
   * 每条会话**最后一帧进度的到达时刻**（`Date.now()`），没有则该 key 不存在。
   *
   * 它守的是一条时效不变量：**这里有值 ⟺ `progress` 里那一帧还在保鲜期内**
   * （`PROGRESS_STALE_MS`，判据来自 shared-view，三端同一份）。速率与剩余时间只有在这
   * 成立时才拿得出来给人看。
   *
   * ## 为什么必须由前端记时刻
   *
   * 内核的 `ProgressTracker::speed()` 确实会在样本老于滑窗时归零，于是**停滞之后的下一帧**
   * 会诚实地带上 `speed: 0` / `eta: null`。问题是**可能根本没有下一帧**：进度事件从收块
   * 路径上发出，传输域里没有任何自走的 tick，对端一安静，最后那帧就永远躺在 `progress` 里
   * ——界面把一个早已不成立的「剩余 45s」一直显示到会话超时。ETA 不是在传输出问题时消失，
   * 而是在传输出问题时**撒谎**。
   *
   * ⚠️ **光记时刻不够**：停滞时没有新事件就没有重渲染，界面会永远停在最后一帧画好的样子上。
   * 所以每帧都重置一个 `PROGRESS_STALE_MS` 的定时器（见 `armStaleTimer`），到点把这个 key
   * 抹掉——那次 setState 就是让订阅者重算的那一下。
   */
  progressAt: Record<string, number>;
  /**
   * 每条会话**此刻正在保存哪个文件**（暂存 → 用户可见位置），没有则该 key 不存在。
   *
   * 「收到的字节数」不等于「文件已保存」：接收是两段式，最后一帧进度把条打到 100% 之后
   * 才开始发布。浏览器上这一段是 OPFS `close()`（O(1)），所以这个域大多数时候一闪而过——
   * 但它存在的理由与 Android 那边一样：**满格的进度条后面跟着一段没有解释的静止**，
   * 是用户读作「卡死」的形状。
   *
   * **一条会话最多一个条目**：发布是「收齐即发布」，接收 actor 串行地做，不会有两个文件
   * 同时在发布。所以这里存的是「当前那一个」而不是一张表。
   *
   * ⚠️ **它必须被清干净**，不能学 `progress` 那样只增不清（那个域留下的陈旧采样已经
   * 造成过「已完成 · 23%」）。清理有两条路：`finished` 事件，以及会话转入非 active 的
   * projection —— 后者兜住失败 / 暂停 / 取消这些「发布还没说完就没了」的路径。
   *
   * ⚠️ **写入比 `started` 事件晚 `PUBLISH_VISIBLE_AFTER_MS`**（见 `schedulePublishReveal`）。
   * 浏览器这一段是 OPFS `close()`，`started` 与 `finished` 背靠背到达却是两条独立事件、
   * 两次渲染，立刻落域就会让进度条每收齐一个文件闪一下灰——收一个几百个小文件的目录时
   * 是持续频闪。真正需要解释的发布（Android SAF 全量拷贝）都远长于这个阈值，延迟揭示
   * 不会漏掉它们。
   */
  publishingBySession: Record<string, FilePublishEvent>;
  /** 最近若干条原始事件，dev 可见；证明 12 种事件全部接住。 */
  eventLog: WebTransferEvent[];

  // —— inbox 域 ——
  /**
   * 收件箱条目（内核 `inbox_items()` 的快照，一次调用即带文件行与传输投影）。
   *
   * **不再由 projections 派生**：收件箱在内核侧已是一张独立的 IndexedDB 表，
   * 不参与传输历史的 `HISTORY_CAP` 淘汰，也不随「清空历史」消失。靠过滤投影拼出来的
   * 旧做法在那两件事上都会说谎。
   *
   * 排序（`receivedAt` 倒序）在 `setInboxItems` 写入时做完，不放 selector——那里派生新数组
   * 会无限重渲染（`pnpm check:zustand-access` 现在会拦）。
   */
  inboxItems: InboxItemDetail[];
  /**
   * 收件箱失效票据：内容可能变了，该重拉一次。**两个生产者**——远端来的
   * （接收方向的 `transferCompleted`）与本机改的（已读 / 归档 / 删除，走 `refreshInbox()`）。
   *
   * 收件箱是低频写入，不值得为它新造一条订阅式推送通道（一次 `inbox_items()` 的成本
   * 远低于此）。面板 effect 依赖这个计数器即可「挂载拉一次 + 此后每次失效重拉」，
   * 拉取点因此始终只有一处，`include_archived` 这个视图状态也不必泄漏进 store。
   */
  inboxRevision: number;
  /**
   * 检索结果条数上限（内核 `inbox_search_limit()`，= 三端共享的 `INBOX_SEARCH_LIMIT`）。
   * `null` = 尚未从 wasm 模块读到。
   *
   * **前端不传这个数**——`search_inbox` 的 limit 缺省就是它。存它只为一件事：UI 要说
   * 「只显示了最近 N 条」时得知道 N 是几。自己写一个 50 就又回到 #111 修掉的那种分叉了。
   */
  inboxSearchLimit: number | null;

  // —— pairing 域 ——
  /** 入站配对请求（browser-as-inviter：桌面消费本机 invite 后到达）。轮询累积。 */
  pendingPairings: PendingPairingJson[];
  /** 已配对设备清单（#77）。轮询快照，非事件驱动——`paired_devices()` 是同步查询非事件流。 */
  pairedDevices: Device[];

  /**
   * 已连接的 **SwarmDrop 客户端**数（`connected_peers()` 的轮询快照，与桌面/移动的
   * `NetworkStatus.connected_peers` 同一个函数）。
   *
   * **与 `pairedDevices` 里 `status === "online"` 的台数不是一回事**：这个数含**未配对**
   * 的对端，那个只数已配对的。两者都不含 bootstrap/relay（内核按 `is_swarmdrop_agent`
   * 过滤掉了）。设置页的设备信息卡两项都摆，因为它们回答的是两个问题。
   */
  connectedPeers: number;

  // —— connection 域（#76）——
  /** 最近一次 `connect()` 成功的结果——浏览器不 listen socket，这只是「拨出去」的连接。 */
  connection: ConnectionJson | null;
  /**
   * 全部基础设施关系的状态快照，来自 `infra_links()` / `infra_changed()`
   * （见 `_lib/infra-watch.ts`）。
   *
   * 它有三个跨路由的消费者：设置页「引导节点」区列清单与移除、设备页「配对」区读其中的
   * circuit 地址判断能不能生成邀请、以及常驻的节点状态徽章（整体健康度）。
   * **可达地址是从这里派生的**（`selectReservation`），不再单独存一份——两份会在多条
   * 引导节点时对不上（旧实现是循环回填，只留最后一条）。
   */
  infraLinks: InfraLink[];
}

const initialState: WebNodeState = {
  status: "idle",
  nodeId: null,
  startedAt: null,
  error: null,
  secure: null,
  deviceName: null,
  deviceNameFallback: null,
  projections: {},
  offers: {},
  rejections: {},
  activePrepare: null,
  clearedPreparedId: null,
  progress: {},
  progressAt: {},
  publishingBySession: {},
  eventLog: [],
  inboxItems: [],
  inboxRevision: 0,
  inboxSearchLimit: null,
  pendingPairings: [],
  pairedDevices: [],
  connectedPeers: 0,
  connection: null,
  infraLinks: [],
};

export const webNodeStore = createStore<WebNodeState>(() => initialState);

/**
 * React 侧订阅入口。
 *
 * **selector 只许返回原始值或 store 内的稳定引用**——`map`/`filter`/`slice` 或对象字面量
 * 每次都是新引用，`useSyncExternalStore` 于是每次快照都不等，直接无限重渲染。派生放组件体内
 * 的 `useMemo`。`pnpm check:zustand-access` 现在覆盖本目录（它此前只扫仓库根 `src/`）。
 */
export function useWebNode<U>(selector: (state: WebNodeState) => U): U {
  return useStore(webNodeStore, selector);
}

// ── actions ────────────────────────────────────────────────────────────────
//
// 「内容没变」一律 `return s`（返回 state 本身），**不要 `return {}`**。
// zustand 的 setState 判的是 `Object.is(partial, state)`：`{}` 是个新对象，判不等 → 照常
// 广播一轮，所有 selector 白求值一次。返回 `s` 才真正短路。
//
// 这条与自研 store 的行为不同（那份实现逐键浅比较，`{}` 天然不通知），迁移时 6 处全部改过。
// 空对象在 zustand 下不报错、类型也合法，纯靠这条约定守。

/**
 * 本机当前的 circuit 可达地址（任意一条已 active 的 relay 给出），没有则 `null`。
 *
 * **是派生而非独立字段**：此前它单独存一份，由启动时的 `infra_until_active` 回填，多条
 * 引导节点时循环覆写只留最后一条——一条挂了就可能显示成「不可达」，而另一条明明是好的。
 * 从清单里现取则多一条少一条都对得上。
 *
 * selector 返回的是**字符串**，符合「不在 selector 里派生新数组/对象」那条约束
 * （`Object.is` 对相同字符串为真，不会每帧换引用）。
 */
export function selectReservation(s: WebNodeState): string | null {
  for (const link of s.infraLinks) {
    if (link.relay?.kind === "active") return link.relay.circuitAddr;
  }
  return null;
}

/**
 * 待处理入站 offer 数（收件箱徽标）与进行中的会话数（设备页的「活跃传输」区块）。
 *
 * 两个都**返回数字**，所以放在 selector 里是安全的（`Object.is(3, 3)` 为真）——同一条规矩
 * 的反面是 `Object.values(...)`，那会每帧换引用、直接打穿 `useSyncExternalStore`。
 *
 * 收在这里而不是各组件内联一份 lambda：内联的 `(s) => Object.keys(s.offers).length` 每次
 * 渲染都是新函数，多个调用点也无法共享；更要紧的是「进行中」的判据必须只有一处
 * （`isActiveSession`），否则将来加一个非 terminal 的 phase，两处数字就会对不上。
 */
export function selectOfferCount(s: WebNodeState): number {
  return Object.keys(s.offers).length;
}

export function selectActiveTransferCount(s: WebNodeState): number {
  return Object.values(s.projections).reduce((n, p) => (isActiveSession(p) ? n + 1 : n), 0);
}

export const webNodeActions = {
  setSecure(info: SecureContextInfo) {
    webNodeStore.setState({ secure: info });
  },
  setStatus(status: NodeStatus) {
    webNodeStore.setState({ status });
  },
  /**
   * 节点已就绪。**三个字段一次落定**——身份、本次启动时刻、状态本就是同一件事的三面，
   * 分成三个 action 调用只会多两轮广播，还给「先置 running 后置 id」这种中间态留了口子。
   */
  markRunning(nodeId: string) {
    webNodeStore.setState({ nodeId, startedAt: Date.now(), status: "running" });
  },
  setError(error: WebError | null) {
    webNodeStore.setState((s) => ({ error, status: error ? "error" : s.status }));
  },
  /** 用户设定的设备名；null = 清空（对外回落到 `deviceNameFallback`）。 */
  setDeviceName(deviceName: string | null) {
    webNodeStore.setState({ deviceName });
  },
  setDeviceNameFallback(deviceNameFallback: string | null) {
    webNodeStore.setState({ deviceNameFallback });
  },
  setInboxSearchLimit(inboxSearchLimit: number) {
    webNodeStore.setState({ inboxSearchLimit });
  },
  /**
   * 事件源一：把一条 transfer 事件归约进对应域，并安排它的**时间副作用**。
   *
   * 两步是分开的：`reduceEvent` 保持纯函数（同一份 state + 同一条事件恒得同一个结果），
   * 「几百毫秒后才揭示」「六秒后作废」这类只有钟表说了算的事全在 `scheduleTimers` 里。
   * 把它们混进 reducer 就等于让归约结果依赖调用时刻，测试与回放都会失去可判定性。
   */
  applyEvent(event: WebTransferEvent) {
    webNodeStore.setState((s) => reduceEvent(s, event));
    scheduleTimers(event);
  },
  /**
   * 事件源三：`transfer_history()` 的一次性回补（#81 跨刷新持久化）。
   *
   * 刷新后事件流从零开始，而 IndexedDB 里还留着传输历史与接收侧续传上下文
   * （收件箱走自己那张表，不经这里）。
   * 已存在的 sessionId **不覆盖**——回补在 `startEventConsumption` 之前调用，理论上撞不上，
   * 但真撞上时实时事件必然比落库快照新。
   */
  setHistory(history: TransferProjection[]) {
    // 早返回不是可选的：`{...}` 必产生新引用，空数组也会白掉一次全局重渲染。
    if (history.length === 0) return;
    webNodeStore.setState((s) => ({
      // 后置展开即「已存在的不覆盖」。
      projections: { ...Object.fromEntries(history.map((p) => [p.sessionId, p])), ...s.projections },
    }));
  },
  /**
   * #104：一条传输记录已在内核侧删除，从投影域摘掉。
   *
   * 与取消/续传不同，删除**没有回流事件**可等——那条会话已经不存在了，内核不会再为它发
   * projection。所以这里是前端唯一的状态更新点，且只该在导出调用成功后调。
   */
  removeProjection(sessionId: string) {
    // 记录都要没了，挂在它上面的两个定时器自然也不该再醒来（醒来只会去改一条不存在的会话）。
    clearSessionTimers(sessionId);
    webNodeStore.setState((s) => {
      if (!(sessionId in s.projections)) return s;
      const projections = { ...s.projections };
      delete projections[sessionId];
      // 三张按 sessionId 索引的表要一起删。`progressAt` 尤其不能留：它的不变量是
      // 「这里有值 ⟺ 那一帧还在保鲜期内」，而保鲜期定时器刚被上面撤掉了 —— 再没有人会来
      // 抹掉这个 key，它会带着一个过期时间戳永久留下，随删除次数线性增长。
      // （桌面 `deleteHistoryItem` 与移动端一直是三张一起删，Web 此前少了这一刀。）
      const { [sessionId]: _frame, ...progress } = s.progress;
      const { [sessionId]: _at, ...progressAt } = s.progressAt;
      const { [sessionId]: _publishing, ...publishingBySession } = s.publishingBySession;
      return { projections, progress, progressAt, publishingBySession };
    });
  },
  /**
   * #104：清空已结束的记录。判据与内核 `clear_transfer_history()` 一致（只删 terminal），
   * 进行中与已中断的一条不动——否则界面会比库先一步「清干净」，刷新又冒回来。
   */
  clearTerminalProjections() {
    webNodeStore.setState((s) => {
      const kept = Object.entries(s.projections).filter(([, p]) => p.phase !== "terminal");
      if (kept.length === Object.keys(s.projections).length) return s;
      const projections = Object.fromEntries(kept);
      // 同 `removeProjection`：三张按 sessionId 索引的表跟着记录一起走，否则 `progressAt`
      // 会留下一批再也没人抹的过期时间戳（保鲜期定时器随即被下面的 retain 撤掉）。
      const survives = (id: string) => id in projections;
      return {
        projections,
        progress: pickByKey(s.progress, survives),
        progressAt: pickByKey(s.progressAt, survives),
        publishingBySession: pickByKey(s.publishingBySession, survives),
      };
    });
    // 定时器不是状态，收在 setState 之外（同 `removeProjection`）。这是它的**批量**版：
    // 一次清掉几十条记录，逐条 `cancel` 要调用点自己记住删了哪些，`retain` 直接按「还在册
    // 的会话」过一遍两张表。
    //
    // 它守的不变量是**每条在途定时器都对应一条在册的会话**。稳态下这里通常一条也撤不掉
    // （会话转终态时 `scheduleTimers` 已经收过），所以这条更像 `removeProjection` 那道清理
    // 的批量对偶——三端此前只有移动端有它，而这台机器的正确性完全靠「每个清理出口都记得」。
    // 漏一处的症状是「一个早已结束的会话过几秒突然又长出一条横幅」，只在特定时序下复现。
    //
    // 反过来「不在册」的定时器一律撤掉也是对的：两个域的四处渲染点都以一条 projection 为
    // 入口，没有 projection 就没有任何东西会读它们。
    const live = webNodeStore.getState().projections;
    retainSessionTimers((sessionId) => sessionId in live);
  },
  /**
   * 收件箱快照落域。**排序在这里做完**，selector 只返回这个稳定引用。
   *
   * 内容未变时不换引用。它守的**不是**「接收完成后的重拉」——那次内容真的多了一条，本就该
   * 换引用；守的是那几次拿回同一份内容的重拉：StrictMode 下 InboxPanel 的 effect
   * double-invoke（同一份快照灌两遍）、收件箱为空时的首次拉取（`[]` → `[]`）、以及同一
   * 会话重复终态事件把 `inboxRevision` 顶两下。少了它这些都各白掉一次全局重渲染
   * ——`[...items].sort()` 每次都是新数组引用，zustand 的 `Object.is` 拦不住。
   */
  setInboxItems(items: InboxItemDetail[]) {
    const next = [...items].sort((a, b) => b.receivedAt - a.receivedAt);
    webNodeStore.setState((s) => (inboxItemsEqual(s.inboxItems, next) ? s : { inboxItems: next }));
  },
  /**
   * 本机改动了收件箱（已读 / 归档 / 删除）后请求重拉。
   *
   * 与 `transferCompleted` 顶同一个计数器是刻意的：面板那条 effect 因此仍是**唯一**的拉取点，
   * 而不是「事件走 effect、本机动作各自再 fetch 一遍」的两套。少了它，动作后要么各写一份
   * `inbox_items()` + `setInboxItems()`，要么就得把 `include_archived` 这个当前视图状态
   * 泄漏进 store。
   *
   * ⚠️ 它是**失效通知不是拉取**：没有收件箱面板挂载时顶计数器不会发生任何事，下次面板挂载
   * 时那条 effect 本来也会拉。目前唯一的消费者就是它，别在其它页面把这个当「刷新收件箱」用。
   */
  refreshInbox() {
    webNodeStore.setState((s) => ({ inboxRevision: s.inboxRevision + 1 }));
  },
  /**
   * 收掉进度行。**无参**：`send_files()` 内部生成 `preparedId`、不回传给调用方，所以
   * 调用点根本拿不到它。三端同此签名（另两端是「失败路径上拿不到」，同一个理由）。
   *
   * 必须挂在 settle 而不是 success 上——准备失败时不收，进度行会永久停在半路。
   */
  clearPrepare() {
    webNodeStore.setState((s) =>
      // 「内容没变」必须 return s，**不能返回 `{}`**——后者是新对象，`Object.is` 判不等，
      // 照样广播一轮。
      s.activePrepare === null
        ? s
        : { activePrepare: null, clearedPreparedId: s.activePrepare.preparedId },
    );
  },
  /** #79：offer 已被本机接受/拒绝，从「待处理」域移除（决策是一次性动作，同 removePendingPairing）。 */
  removeOffer(sessionId: string) {
    webNodeStore.setState((s) => {
      if (!(sessionId in s.offers)) return s;
      const offers = { ...s.offers };
      delete offers[sessionId];
      return { offers };
    });
  },
  /**
   * `pending_offers()` 是只读快照，用于补回事件流接管前已经到达的请求；之后继续由
   * `transferOfferReceived` 事件实时驱动。每次轮询都以内核的 pending 集合为准，从而不会
   * 留下已经在别处接受/拒绝的陈旧条目。
   */
  setPendingOffers(offers: OfferJson[]) {
    const next = Object.fromEntries(offers.map((offer) => [offer.sessionId, offerFromSnapshot(offer)]));
    webNodeStore.setState((s) => (offersEqual(s.offers, next) ? s : { offers: next }));
  },
  /** 事件源二：轮询到的入站配对请求，累积（内核侧取出即清空，故这里追加不去重覆盖）。 */
  addPendingPairings(reqs: PendingPairingJson[]) {
    if (reqs.length === 0) return;
    webNodeStore.setState((s) => ({ pendingPairings: [...s.pendingPairings, ...reqs] }));
  },
  removePendingPairing(pendingId: string) {
    webNodeStore.setState((s) => ({
      pendingPairings: s.pendingPairings.filter((r) => r.pendingId !== pendingId),
    }));
  },
  /**
   * 每 1.5s 轮询都会传入一个新数组引用（`paired_devices()` 每次现造），若跳过内容比较，
   * 订阅者会无谓重渲染。DashMap 遍历顺序不保证稳定，比较必须与顺序无关。
   */
  setPairedDevices(devices: Device[]) {
    webNodeStore.setState((s) => (devicesEqual(s.pairedDevices, devices) ? s : { pairedDevices: devices }));
  },
  /** 内容没变时 `return s`——不是 `return {}`：后者是新对象，`Object.is` 判不等，照样广播一轮。 */
  setConnectedPeers(connectedPeers: number) {
    webNodeStore.setState((s) => (s.connectedPeers === connectedPeers ? s : { connectedPeers }));
  },
  setConnection(connection: ConnectionJson | null) {
    webNodeStore.setState({ connection });
  },
  setInfraLinks(infraLinks: InfraLink[]) {
    webNodeStore.setState({ infraLinks });
  },
  /**
   * 关停后清空运行态，保留已探测的 secure 结果（环境不因关节点而改变）。
   * 设备名同理：它持久化在 IndexedDB、回退名由 UA 派生，两者都不是节点的运行态。
   *
   * `inboxSearchLimit` 更不是——它是**编译期常量**的镜像。而重读它的唯一入口只在 layout
   * mount 跑一次，抹成 `null` 就再也回不来，「只显示最近 N 条」那条提示会永久消失。
   */
  reset() {
    // 节点停了就不会再有任何事件，两张定时器表里剩下的全是空转——留着它们只会在几百毫秒
    // 后往一个已经清空的 store 里写回「正在保存」。
    clearAllTimers();
    webNodeStore.setState((s) => ({
      ...initialState,
      secure: s.secure,
      deviceName: s.deviceName,
      deviceNameFallback: s.deviceNameFallback,
      inboxSearchLimit: s.inboxSearchLimit,
    }));
  },
};

// ── 时间副作用（定时器）────────────────────────────────────────────────────────
//
// 这里只有两件事，共同点是**都不可能由事件驱动**：
//
// - 「这一帧进度过期了」——进度事件只从收块路径发出，对端一安静就再没有下一帧，
//   于是也不会有下一次渲染。没有定时器，界面会永远停在最后一帧画好的样子上。
// - 「这次发布值得说一句」——`started` 与 `finished` 可能背靠背到达（OPFS `close()`），
//   急着渲染就是频闪。要等一等才知道这次发布是不是真的慢到需要解释。
//
// 两个阈值都来自 `@swarmdrop/shared-view`，**不在本端另写一个数**：三端各有一份渲染代码，
// 判据分家就会变成一端 6 秒、一端 10 秒、一端忘了做。

// 台账原语来自 `@swarmdrop/shared-view`（`createSessionTimers`），**本端不再手写两张 Map**：
// 「一个会话至多一条在途定时器」「重排先撤旧的」「回调里先摘台账再执行」这几条，三端此前
// 各写了一份并且已经漂——批量清理只有移动端有、桌面把它内联展开、Web 干脆没有。
// 调度器是参数，因为那个包零平台依赖（`setTimeout` 在它的 tsconfig 里根本不存在）。
//
// ⚠️ **必须包一层箭头，不能直接把 `setTimeout` / `clearTimeout` 当值传进去。** 两个理由：
// ①这两个台账是模块级常量，直接传等于**在模块求值那一刻**把当时的全局函数快照下来——
//   `vi.useFakeTimers()` 之后换掉的是 `globalThis.setTimeout`，快照里的还是真钟，于是
//   store.test.ts 里所有靠假时钟推进的用例会一起挂（本次改造当场踩到）；
// ②浏览器里 `setTimeout` 是 WebIDL 方法，脱离 `window` 单独调用会 `Illegal invocation`。
//   箭头包一层则是每次调用现从全局作用域取，两条都躲开了。

/** 每条会话最多一个保鲜期定时器（到点抹掉 `progressAt[sessionId]`）。 */
const staleTimers = createSessionTimers(
  (fire, delayMs) => setTimeout(fire, delayMs),
  (handle) => clearTimeout(handle),
);
/** 每条会话最多一个发布揭示定时器（到点才把 `started` 落进 `publishingBySession`）。 */
const publishRevealTimers = createSessionTimers(
  (fire, delayMs) => setTimeout(fire, delayMs),
  (handle) => clearTimeout(handle),
);

function clearSessionTimers(sessionId: string) {
  staleTimers.cancel(sessionId);
  publishRevealTimers.cancel(sessionId);
}

/** 批量清理：只留 `isLive` 认可的会话，其余一律撤掉（记录被成批删除后用）。 */
function retainSessionTimers(isLive: (sessionId: string) => boolean) {
  staleTimers.retain(isLive);
  publishRevealTimers.retain(isLive);
}

function clearAllTimers() {
  staleTimers.clear();
  publishRevealTimers.clear();
}

/**
 * 重起这条会话的保鲜期定时器：到点把到达时刻抹掉。
 *
 * **那次 setState 就是「让订阅者重算」的那一下**——不是顺手清理。停滞时没有任何别的东西
 * 会触发重渲染，少了它，`usableRates` 写得再对也永远没有机会被重新求值。
 */
function armStaleTimer(sessionId: string) {
  staleTimers.schedule(sessionId, PROGRESS_STALE_MS, () => {
    webNodeStore.setState((s) => {
      // 「内容没变」一律 return s，不能返回 `{}`（新对象 → `Object.is` 判不等 → 白广播）。
      if (!(sessionId in s.progressAt)) return s;
      const { [sessionId]: _stale, ...progressAt } = s.progressAt;
      return { progressAt };
    });
  });
}

/**
 * 延迟揭示一次发布。撑过 `PUBLISH_VISIBLE_AFTER_MS` 才落域——常数时间的发布压根等不到
 * 这一刻（`finished` 或会话状态转换会先把定时器取消掉），于是界面上不会有那一下灰闪。
 */
/** 按会话 id 过滤一张 `Record<sessionId, T>`，保持「内容没变就还回原引用」。 */
function pickByKey<T>(
  table: Record<string, T>,
  keep: (sessionId: string) => boolean,
): Record<string, T> {
  const kept = Object.entries(table).filter(([id]) => keep(id));
  return kept.length === Object.keys(table).length ? table : Object.fromEntries(kept);
}

function schedulePublishReveal(event: FilePublishEvent) {
  publishRevealTimers.schedule(event.sessionId, PUBLISH_VISIBLE_AFTER_MS, () => {
    webNodeStore.setState((s) => {
      // **会话已经不活跃了就别写。** 这是「凭空长出一条永不消失的正在保存」的唯一入口：
      // 若 `started` 因事件乱序落在会话终态之后，这个定时器仍会在 300ms 后把条目写回，
      // 而该会话再也不会有新的 projection 来清它 —— 条目就永久驻留了。宁可不显示。
      // （移动端一直有这道校验，桌面与 Web 在 2026-08-10 对齐。）
      if (s.projections[event.sessionId]?.phase !== "active") return s;
      return {
        publishingBySession: { ...s.publishingBySession, [event.sessionId]: event },
      };
    });
  });
}

/**
 * 一条事件的时间副作用。**与 `reduceEvent` 的分工**：那边回答「域里现在是什么」，
 * 这边回答「过一会儿还要发生什么」。
 *
 * ⚠️ 取消路径不能漏。发布揭示定时器一旦在会话已经转终态之后才醒来，就会凭空写回一条
 * 永远没人来清的「正在保存」——`finished` 与「转入非 active」两条路都必须取消它，
 * 与 `reduceEvent` 里那两处清理**一一对应**。
 */
function scheduleTimers(ev: WebTransferEvent) {
  switch (ev.type) {
    case "transferProgress":
      armStaleTimer(ev.event.sessionId);
      return;
    case "transferProjection":
      // 非 active 的会话不会再有进度帧，也不会再有发布——两个定时器一起收掉。
      if (ev.projection.phase !== "active") clearSessionTimers(ev.projection.sessionId);
      return;
    case "filePublish":
      if (ev.event.phase === "started") schedulePublishReveal(ev.event);
      else publishRevealTimers.cancel(ev.event.sessionId);
      return;
    default:
      return;
  }
}

// ── event reducer ────────────────────────────────────────────────────────────

/**
 * 把一条 `WebTransferEvent` 归约进对应域，绝不丢弃（未命中的也入 eventLog 留痕）。
 * 结构化落域的有 6 类：projection / offer / progress / prepare / rejection / filePublish，
 * 另加 `transferCompleted` 只推一个收件箱重拉信号（不落数据，真表在内核侧）。
 * **新增需要落域的事件在此加 case。**
 */
function reduceEvent(s: WebNodeState, ev: WebTransferEvent): Partial<WebNodeState> {
  const eventLog = appendLog(s.eventLog, ev);
  switch (ev.type) {
    case "transferProjection": {
      const { sessionId, phase } = ev.projection;
      const patch: Partial<WebNodeState> = {
        projections: { ...s.projections, [sessionId]: ev.projection },
        eventLog,
      };
      // **待决 offer 必须随会话转终态一起消失。**
      //
      // `offers` 此前只由 accept / reject 成功后的 `removeOffer` 清理，于是会话被**动**
      // 结束时那条 offer 会永久留在队列里：全局对话框反复弹一条已经死掉的请求，
      // 点「接受」只会撞上内核的「会话不存在」。收件箱的「待处理请求」区让它更持久
      // ——那是一条常驻列表，不像弹窗还能被关掉。
      //
      // 被动结束的三条真实来路：对端取消、对端下线、**本端决策窗口耗尽**
      // （`PENDING_OFFER_TIMEOUT_SECS`，内核清理任务发 `TimeoutSignal::OfferExpired`
      // 把会话推成 `terminal(expired)`）。最后一条是最常撞上的：170 秒没人管就到期，
      // 而在内核补上那条终态之前，这里写的清理**从来没有被触发过**。
      //
      // projection 是生命周期的唯一权威源（内核每次状态转换都重发），所以清理挂在这里，
      // 而不是再去订阅 failed/cancelled 那几个冗余事件。
      if (phase === "terminal" && sessionId in s.offers) {
        const { [sessionId]: _resolved, ...offers } = s.offers;
        patch.offers = offers;
      }
      // 「正在保存」的兜底清理，理由与上面那段同源：`FilePublish` 的 `finished` 只在发布
      // **成功**时到达。发布失败会以可恢复的中断冒泡（`publish_file` 的 `?`），走的是
      // 会话状态转换这条路而不是再发一条发布事件——所以只要会话不再是 active，那条
      // 「正在保存 x.zip」就该收掉。少了这一条，一次失败的接收会永久顶着一句正在保存。
      if (phase !== "active" && sessionId in s.publishingBySession) {
        const { [sessionId]: _aborted, ...publishingBySession } = s.publishingBySession;
        patch.publishingBySession = publishingBySession;
      }
      // 到达时刻同理：非 active 的会话不会再有进度帧，那一帧的速率与剩余时间即刻作废。
      // （渲染侧本就不给非 active 摆这两格，这里是让「有值 ⟺ 还新鲜」这条不变量不留缺口。）
      if (phase !== "active" && sessionId in s.progressAt) {
        const { [sessionId]: _expired, ...progressAt } = s.progressAt;
        patch.progressAt = progressAt;
      }
      return patch;
    }
    case "filePublish": {
      const { sessionId, phase } = ev.event;
      // `started` **在这里不落域**，交给 `schedulePublishReveal` 的延迟揭示（理由写在
      // `publishingBySession` 的注释里：常数时间的发布会让进度条每收齐一个文件闪一下灰）。
      // 它仍然进 eventLog——「12 种事件全部接住」那条不因延迟揭示打折。
      if (phase === "started") return { eventLog };
      // `finished` **按 sessionId 摘，不比 fileId**：发布是收齐即发布、由接收 actor 串行
      // 地做，同一条会话不会有两个文件同时在发布，所以这里存的那一条必然就是刚发布完的
      // 那一个。已经不在了就只留痕（会话可能先一步转了终态，上面那条兜底已经清过）。
      if (!(sessionId in s.publishingBySession)) return { eventLog };
      const { [sessionId]: _done, ...publishingBySession } = s.publishingBySession;
      return { publishingBySession, eventLog };
    }
    case "transferOfferReceived":
      return {
        offers: { ...s.offers, [ev.offer.sessionId]: offerFromEvent(ev.offer) },
        eventLog,
      };
    case "transferProgress":
      // 到达时刻与帧本身一起写：`progress` 只增不清（终态由 `transferSample` 挡），
      // 而 `progressAt` 是有保质期的那一份——保鲜期定时器到点会把它抹掉。
      return {
        progress: { ...s.progress, [ev.event.sessionId]: ev.event },
        progressAt: { ...s.progressAt, [ev.event.sessionId]: Date.now() },
        eventLog,
      };
    case "prepareProgress": {
      // 刚被清掉的批次的迟到事件：丢弃，别让它重新占住活跃位。
      if (ev.event.preparedId === s.clearedPreparedId) return { eventLog };
      const active = s.activePrepare;
      // 让位给新批次的三种情形：没有活跃批次、就是同一批、或者上一批已经跑到 100% 却
      // 没人清。
      const canClaim =
        active === null ||
        active.preparedId === ev.event.preparedId ||
        active.bytesHashed >= active.totalBytes;
      return canClaim ? { activePrepare: ev.event, eventLog } : { eventLog };
    }
    case "transferCompleted":
      // 只有**接收**方向会新增收件箱条目（发送完成不进这本账）。自增计数器让收件箱面板
      // 重拉一次真表——收件箱不再由 projections 派生，projection 事件不足以让它更新。
      return ev.event.direction === "receive"
        ? { inboxRevision: s.inboxRevision + 1, eventLog }
        : { eventLog };
    case "transferRejected":
      // TransferProjection 的 terminalReason 只到 "rejected" 粒度，不含 reason.type（区分
      // not_paired / user_declined / policy_rejected / receiving_paused）——发送侧要给出精确
      // 提示（尤其 #79 验收标准的「未配对」硬拒场景）必须落这个单独的域。
      return { rejections: { ...s.rejections, [ev.event.sessionId]: ev.event }, eventLog };
    default:
      // 其余终态事件（accepted/failed/paused/resumed/dbError）与 TransferProjection
      // 的 phase/terminalReason/errorMessage 冗余（内核每次状态转换重发 projection），基座只留痕；
      // 未知事件（.d.ts 未覆盖的新变体）同样留痕不吞。
      return { eventLog };
  }
}

function appendLog(log: WebTransferEvent[], ev: WebTransferEvent): WebTransferEvent[] {
  const next = log.length >= EVENT_LOG_CAP ? log.slice(1) : log.slice();
  next.push(ev);
  return next;
}

/**
 * 收件箱快照的内容比较（两侧都已按 `receivedAt` 倒序，故可逐位比）。
 *
 * 比的是 UI 真正会变的那几样：条目集合、接收时间、缺失标记、文件数，**以及已读与归档时间戳**。
 *
 * 后两个字段一度不在这里，理由是「归档 / 软删的条目根本不在列表里，无需比时间戳」——那句话
 * 在收件箱只能读不能写的时候成立，#108 给它加了三个写入口之后就成了假设。两条真实的漏网路径：
 * 下载后标已读（集合不变，只有 `lastOpenedAt` 从 null 变成时间戳）、以及「显示已归档」开着时
 * 取消归档（集合同样不变）。两者都会被判等丢掉，于是未读点与「已归档」徽标停在旧值不动。
 *
 * 教训是这个守卫编码的是**当时的域模型**（「有哪些条目」），而 UI 后来开始读「条目带什么状态」。
 * 往 `InboxItemDetail` 上加可变字段时要回来看一眼这里。
 *
 * **`transfer` 投影刻意不比**：那是每次 `inbox_items()` 现造的对象，比它（哪怕只比引用）
 * 会让这个函数恒为 false，守卫退化成纯开销；而本面板压根不渲染它。
 */
function inboxItemsEqual(a: InboxItemDetail[], b: InboxItemDetail[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((item, index) => {
    const other = b[index];
    return (
      item.id === other.id &&
      item.receivedAt === other.receivedAt &&
      item.missing === other.missing &&
      item.files.length === other.files.length &&
      item.lastOpenedAt === other.lastOpenedAt &&
      item.archivedAt === other.archivedAt
    );
  });
}

/** 与顺序无关的内容比较——DashMap 遍历顺序不保证跨调用稳定。 */
function devicesEqual(a: Device[], b: Device[]): boolean {
  if (a.length !== b.length) return false;
  const key = (d: Device) => `${d.peerId}|${d.status}|${d.connection}|${d.latency}`;
  const seen = new Set(a.map(key));
  return b.every((d) => seen.has(key(d)));
}

function offerFromEvent(offer: TransferOfferEvent): IncomingOffer {
  return {
    sessionId: offer.sessionId,
    peerId: offer.peerId,
    deviceName: offer.deviceName,
    totalSize: offer.totalSize,
    files: offer.files.map(({ fileId, name, relativePath, size }) => ({ fileId, name, relativePath, size })),
  };
}

function offerFromSnapshot(offer: OfferJson): IncomingOffer {
  return {
    sessionId: offer.sessionId,
    peerId: offer.peerId,
    deviceName: offer.peerName,
    totalSize: offer.totalSize,
    files: offer.files.map(({ fileId, name, relativePath, size }) => ({ fileId, name, relativePath, size })),
  };
}

/** 轮询快照每次都会新建对象；按可见字段比较后才通知 React，避免 1.5 秒空转重渲染。 */
function offersEqual(a: Record<string, IncomingOffer>, b: Record<string, IncomingOffer>): boolean {
  const aKeys = Object.keys(a);
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((sessionId) => {
    const left = a[sessionId];
    const right = b[sessionId];
    if (!right || left.peerId !== right.peerId || left.deviceName !== right.deviceName || left.totalSize !== right.totalSize) {
      return false;
    }
    return (
      left.files.length === right.files.length &&
      left.files.every((file, index) => {
        const other = right.files[index];
        return (
          file.fileId === other.fileId &&
          file.name === other.name &&
          file.relativePath === other.relativePath &&
          file.size === other.size
        );
      })
    );
  });
}
