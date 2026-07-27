// Web 应用区的类型面：再导出 `swarmdrop-web`（wasm-bindgen 生成的 .d.ts）的 JS 可见类型。
//
// `WebNode` class 与 init（default）都由 wasm-bindgen 生成精确签名，直接复用——不手写镜像，
// 避免第三处类型副本漂移（Rust 侧签名一改，生成的 .d.ts 自动更新，手抄的不会跟）。
// 运行时一律动态 `import("swarmdrop-web")`（见 node-runtime.ts），只 `import type` 不进 bundle。

export type {
  WebNode,
  WebError,
  WebTransferEvent,
  TransferProjection,
  TransferOfferEvent,
  OfferJson,
  TransferRejectedEvent,
  TransferProgressEvent,
  PrepareProgressEvent,
  PendingPairingJson,
  NodeAddrJson,
  ConnectionJson,
  OfferRejectReason,
  PathKindJson,
  Device,
} from "swarmdrop-web";

import type { OfferRejectReason, WebError } from "swarmdrop-web";

/** 动态 import 的模块类型：跟随生成的 .d.ts（含 default=init 与 `WebNode` class，带 static spawn）。 */
export type SwarmdropWebModule = typeof import("swarmdrop-web");

/** `WebError.kind` 的中文标签——错误呈现统一收口用（web-error-view.tsx / connection-panel.tsx 共用）。 */
export const WEB_ERROR_KIND_LABEL: Record<WebError["kind"], string> = {
  identity: "身份错误",
  network: "网络错误",
  transfer: "传输错误",
  invalidInput: "输入无效",
  aborted: "已取消",
  notFound: "未找到",
  storage: "存储错误",
};

/**
 * `OfferRejectReason["type"]` 的中文标签（#79：对端拒绝 offer 的提示，尤其 `notPaired` 那句
 * ——内核安全边界，不是「静默失败」，前端必须区分展示）。
 */
export const OFFER_REJECT_REASON_LABEL: Record<OfferRejectReason["type"], string> = {
  not_paired: "对方尚未与你配对，请先完成配对后再试",
  user_declined: "对方拒绝了此次传输",
  policy_rejected: "对方的接收策略拒绝了此次传输",
  receiving_paused: "对方已暂停接收，请稍后再试",
};

/**
 * 把任意 reject 值收敛成 `WebError`。wasm-bindgen 方法 reject 的就是 `{ kind, message }`；
 * 非该形状（如 JS 运行时异常）兜底成 `network` kind，保证 UI 永远拿到结构化错误。
 */
export function toWebError(e: unknown): WebError {
  if (
    e !== null &&
    typeof e === "object" &&
    "kind" in e &&
    "message" in e &&
    typeof (e as { kind: unknown }).kind === "string"
  ) {
    return e as WebError;
  }
  return {
    kind: "network",
    message: e instanceof Error ? e.message : String(e),
  };
}
