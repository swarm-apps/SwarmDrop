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
  TransferProgressEvent,
  PrepareProgressEvent,
  PendingPairingJson,
} from "swarmdrop-web";

import type { WebError } from "swarmdrop-web";

/** 动态 import 的模块类型：跟随生成的 .d.ts（含 default=init 与 `WebNode` class，带 static spawn）。 */
export type SwarmdropWebModule = typeof import("swarmdrop-web");

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
