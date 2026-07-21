// Web 应用区的状态层：镜像桌面 `src/stores/network-store` 的思路，但事件源是「双轨」的——
//   源一：transfer 域事件走 `events()` 的 ReadableStream（单点消费，见 event-dispatch.ts）；
//   源二：pairing 入站请求 + 已配对设备走同步 getter 轮询（见 state-poll.ts）。
// 二者都汇入本 store。actions 独立于 state（不塞进 state 对象），保证 selector 快照稳定。

import { createStore, useStore } from "./create-store";
import type { SecureContextInfo } from "./secure-context";
import type {
  ConnectionJson,
  Device,
  PendingPairingJson,
  PrepareProgressEvent,
  TransferOfferEvent,
  TransferProgressEvent,
  TransferProjection,
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
 * Worker 模式落地时（对应 OPFS）需改为按运行环境派生。
 */
const IDENTITY_LOCATION = "localStorage（Window 主线程）";

export interface WebNodeState {
  // —— node 域 ——
  status: NodeStatus;
  /** base58 身份；刷新后不变（内核 identity::load_or_create 持久化到 localStorage）。 */
  nodeId: string | null;
  /** 身份持久化位置（Window=localStorage / Worker=OPFS，当前基座恒为前者）。 */
  identityLocation: string;
  error: WebError | null;
  /** secure-context 探测结果；null = 尚未探测（SSR 快照）。 */
  secure: SecureContextInfo | null;

  // —— transfer 域（以 projection 为主状态源）——
  /** 传输投影：前端主状态源（内核逐步以 transferProjection 事件替代分散的终态事件）。 */
  projections: Record<string, TransferProjection>;
  /** 挂起入站 offer（按 sessionId）。 */
  offers: Record<string, TransferOfferEvent>;
  /** 发送侧 prepare（hash + bao outboard）进度，按 preparedId。 */
  prepares: Record<string, PrepareProgressEvent>;
  /**
   * 实时进度：speed / eta / 单文件粒度。**TransferProjection 从不携带这些字段**，故单独建域
   * （projection 只表达 phase + 累计字节，不会取代 progress）。供 #80 传输视图直接消费。
   */
  progress: Record<string, TransferProgressEvent>;
  /** 最近若干条原始事件，dev 可见；证明 11 种事件全部接住。 */
  eventLog: WebTransferEvent[];

  // —— pairing 域 ——
  /** 入站配对请求（browser-as-inviter：桌面消费本机 invite 后到达）。轮询累积。 */
  pendingPairings: PendingPairingJson[];
  /** 已配对设备清单（#77）。轮询快照，非事件驱动——`paired_devices()` 是同步查询非事件流。 */
  pairedDevices: Device[];

  // —— connection 域（#76）——
  /** 最近一次 `connect()` 成功的结果——浏览器不 listen socket，这只是「拨出去」的连接。 */
  connection: ConnectionJson | null;
  /**
   * 最近一次 `reserve()` 成功拿到的 circuit 可达地址。浏览器唯一的被动接收入口，
   * #77（配对）生成邀请前需要它——存进 store 而非局部 state，避免 #77 重复 reserve。
   */
  reservation: string | null;
}

const initialState: WebNodeState = {
  status: "idle",
  nodeId: null,
  identityLocation: IDENTITY_LOCATION,
  error: null,
  secure: null,
  projections: {},
  offers: {},
  prepares: {},
  progress: {},
  eventLog: [],
  pendingPairings: [],
  pairedDevices: [],
  connection: null,
  reservation: null,
};

export const webNodeStore = createStore<WebNodeState>(initialState);

/** React 侧订阅入口。selector 只选原始值或 store 内稳定引用（见 create-store 注释）。 */
export function useWebNode<U>(selector: (state: WebNodeState) => U): U {
  return useStore(webNodeStore, selector);
}

// ── actions ────────────────────────────────────────────────────────────────

export const webNodeActions = {
  setSecure(info: SecureContextInfo) {
    webNodeStore.setState({ secure: info });
  },
  setStatus(status: NodeStatus) {
    webNodeStore.setState({ status });
  },
  setNodeId(nodeId: string) {
    webNodeStore.setState({ nodeId });
  },
  setError(error: WebError | null) {
    webNodeStore.setState((s) => ({ error, status: error ? "error" : s.status }));
  },
  /** 事件源一：把一条 transfer 事件归约进对应域。 */
  applyEvent(event: WebTransferEvent) {
    webNodeStore.setState((s) => reduceEvent(s, event));
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
    webNodeStore.setState((s) => (devicesEqual(s.pairedDevices, devices) ? {} : { pairedDevices: devices }));
  },
  setConnection(connection: ConnectionJson | null) {
    webNodeStore.setState({ connection });
  },
  setReservation(reservation: string | null) {
    webNodeStore.setState({ reservation });
  },
  /** 关停后清空运行态，保留已探测的 secure 结果（环境不因关节点而改变）。 */
  reset() {
    webNodeStore.setState((s) => ({ ...initialState, secure: s.secure }));
  },
};

// ── event reducer ────────────────────────────────────────────────────────────

/**
 * 把一条 `WebTransferEvent` 归约进对应域，绝不丢弃（未命中的也入 eventLog 留痕）。
 * 结构化落域的只有 4 类：projection / offer / progress / prepare。**新增需要落域的事件在此加 case。**
 */
function reduceEvent(s: WebNodeState, ev: WebTransferEvent): Partial<WebNodeState> {
  const eventLog = appendLog(s.eventLog, ev);
  switch (ev.type) {
    case "transferProjection":
      return {
        projections: { ...s.projections, [ev.projection.sessionId]: ev.projection },
        eventLog,
      };
    case "transferOfferReceived":
      return { offers: { ...s.offers, [ev.offer.sessionId]: ev.offer }, eventLog };
    case "transferProgress":
      return { progress: { ...s.progress, [ev.event.sessionId]: ev.event }, eventLog };
    case "prepareProgress":
      return { prepares: { ...s.prepares, [ev.event.preparedId]: ev.event }, eventLog };
    default:
      // 终态事件（accepted/rejected/completed/failed/paused/resumed/dbError）与 TransferProjection
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

/** 与顺序无关的内容比较——DashMap 遍历顺序不保证跨调用稳定。 */
function devicesEqual(a: Device[], b: Device[]): boolean {
  if (a.length !== b.length) return false;
  const key = (d: Device) => `${d.peerId}|${d.status}|${d.connection}|${d.latency}`;
  const seen = new Set(a.map(key));
  return b.every((d) => seen.has(key(d)));
}
