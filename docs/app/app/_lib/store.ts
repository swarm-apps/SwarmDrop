// Web 应用区的状态层：镜像桌面 `src/stores/network-store` 的思路，但事件源是「三轨」的——
//   源一：transfer 域事件走 `events()` 的 ReadableStream（单点消费，见 event-dispatch.ts）；
//   源二：pairing 入站请求、挂起 offer + 已配对设备走同步 getter 轮询（见 state-poll.ts）；
//   源三：`transfer_history()` 在 spawn 后一次性回补 IndexedDB 里的历史投影（收件箱 + 活动
//         视图跨刷新，见 web-node-bootstrap.tsx）。
// 三者都汇入本 store。actions 独立于 state（不塞进 state 对象），保证 selector 快照稳定。

import { createStore, useStore } from "./create-store";
import type { SecureContextInfo } from "./secure-context";
import type {
  ConnectionJson,
  Device,
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
 * Worker 模式落地时（对应 OPFS）需改为按运行环境派生。
 */
export const IDENTITY_LOCATION = "localStorage（Window 主线程）";

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
  error: WebError | null;
  /** secure-context 探测结果；null = 尚未探测（SSR 快照）。 */
  secure: SecureContextInfo | null;

  // —— transfer 域（以 projection 为主状态源）——
  /**
   * 传输投影：前端主状态源（内核逐步以 transferProjection 事件替代分散的终态事件）。
   * 收件箱（`direction=receive` 且 `terminalReason=completed`）与传输活动视图都由它派生，
   * 故 #81 的跨刷新持久化只需在启动时把 `transfer_history()` 回补进来（见 `setHistory`）。
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
  /** 发送侧 prepare（hash + bao outboard）进度，按 preparedId。 */
  prepares: Record<string, PrepareProgressEvent>;
  /**
   * 最近一条 prepare 进度事件（#78 发送面板用：`send_files()` 内部生成的 preparedId 不回传
   * 给调用方，MVP 只支持单个活跃发送，故用「最近一条」代表当前发送的 prepare 阶段）。
   */
  latestPrepareProgress: PrepareProgressEvent | null;
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
  error: null,
  secure: null,
  projections: {},
  offers: {},
  rejections: {},
  prepares: {},
  latestPrepareProgress: null,
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
  /**
   * 事件源三：`transfer_history()` 的一次性回补（#81 跨刷新持久化）。
   *
   * 刷新后事件流从零开始，而 IndexedDB 里还留着收件箱、传输历史与接收侧续传上下文。
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
  /** #79：offer 已被本机接受/拒绝，从「待处理」域移除（决策是一次性动作，同 removePendingPairing）。 */
  removeOffer(sessionId: string) {
    webNodeStore.setState((s) => {
      if (!(sessionId in s.offers)) return {};
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
    webNodeStore.setState((s) => (offersEqual(s.offers, next) ? {} : { offers: next }));
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
      return {
        offers: { ...s.offers, [ev.offer.sessionId]: offerFromEvent(ev.offer) },
        eventLog,
      };
    case "transferProgress":
      return { progress: { ...s.progress, [ev.event.sessionId]: ev.event }, eventLog };
    case "prepareProgress":
      return {
        prepares: { ...s.prepares, [ev.event.preparedId]: ev.event },
        latestPrepareProgress: ev.event,
        eventLog,
      };
    case "transferRejected":
      // TransferProjection 的 terminalReason 只到 "rejected" 粒度，不含 reason.type（区分
      // not_paired / user_declined / policy_rejected / receiving_paused）——发送侧要给出精确
      // 提示（尤其 #79 验收标准的「未配对」硬拒场景）必须落这个单独的域。
      return { rejections: { ...s.rejections, [ev.event.sessionId]: ev.event }, eventLog };
    default:
      // 其余终态事件（accepted/completed/failed/paused/resumed/dbError）与 TransferProjection
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
