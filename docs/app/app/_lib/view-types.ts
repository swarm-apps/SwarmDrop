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
  PairInvitePreviewJson,
  ConnectionJson,
  OfferRejectReason,
  PathKindJson,
  RelayInfoJson,
  RelayStateKind,
  Device,
  DeviceTrustLevel,
  DeviceReceivePolicy,
  InboxItemDetail,
  InboxItemSummary,
  InboxItemFileEntry,
  InboxSearchHit,
  InboxHitFile,
} from "swarmdrop-web";

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
// 上面那段是**纯再导出**（`export type {...}`），本模块自己用不到那些名字——
// 下面的标签映射要按枚举取值建 Record，所以另 import 一次。
import type { InboxItemSummary, OfferRejectReason, WebError } from "swarmdrop-web";
import type { TimeBucketKey } from "@swarmdrop/shared-view";

/** 动态 import 的模块类型：跟随生成的 .d.ts（含 default=init 与 `WebNode` class，带 static spawn）。 */
export type SwarmdropWebModule = typeof import("swarmdrop-web");

/**
 * `WebError.kind` 的标签——错误呈现统一收口用（web-error-view.tsx / connection-panel.tsx 共用）。
 *
 * 值是**可翻译描述符**而不是字符串：本模块是纯类型/常量层，翻译宏在这里只能定义、不能展开
 * （展开要 `useLingui()`，那是组件的事）。消费方拿到描述符后自己 `t(...)`。
 */
export const WEB_ERROR_KIND_LABEL: Record<WebError["kind"], MessageDescriptor> = {
  identity: msg`身份错误`,
  network: msg`网络错误`,
  transfer: msg`传输错误`,
  invalidInput: msg`输入无效`,
  aborted: msg`已取消`,
  notFound: msg`未找到`,
  storage: msg`存储错误`,
};

/**
 * `OfferRejectReason["type"]` 的中文标签（#79：对端拒绝 offer 的提示，尤其 `notPaired` 那句
 * ——内核安全边界，不是「静默失败」，前端必须区分展示）。
 */
export const OFFER_REJECT_REASON_LABEL: Record<OfferRejectReason["type"], MessageDescriptor> = {
  not_paired: msg`对方尚未与你配对，请先完成配对后再试`,
  user_declined: msg`对方拒绝了此次传输`,
  policy_rejected: msg`对方的接收策略拒绝了此次传输`,
  receiving_paused: msg`对方已暂停接收，请稍后再试`,
  // 合法客户端永远不会触发它：这条 offer 里有文件路径会逃出对方的保存目录。
  // 措辞指向「客户端有问题」而不是「对方设置有问题」，因为后者会让用户去改一个
  // 改不好的设置。
  unsafe_path: msg`此次传输被对方判定为不合法（文件路径异常），请检查客户端版本`,
};

/**
 * 收件箱条目的**来源身份**与**内容类型**。两者都是 DTO 里一直有、Web 端此前从没读过的字段。
 *
 * 存描述符不存字符串：本模块是 `_lib/` 下的纯数据，翻译宏在这里只能定义、不能展开
 * （展开要 `useLingui()`，那是组件的事）。调用点拿到描述符自己 `t(...)`。
 */
export const INBOX_SOURCE_KIND_LABEL: Record<InboxItemSummary["sourceKind"], MessageDescriptor> = {
  paired_device: msg`已配对设备`,
  share_code: msg`配对码`,
  mcp: msg`AI 代理`,
  unknown: msg`来源未知`,
};

export const INBOX_CONTENT_KIND_LABEL: Record<InboxItemSummary["contentKind"], MessageDescriptor> = {
  files: msg`文件`,
  text: msg`文本`,
  clipboard: msg`剪贴板`,
  bundle: msg`打包内容`,
};

/**
 * 时间分组的组头文案。分桶逻辑本身在 `@swarmdrop/shared-view`（`groupByTimeBucket`），
 * 它只给判别式 key——**文案不进那个包**，那是各端本地化的事（同该包 README 的归属判据）。
 */
export const TIME_BUCKET_LABEL: Record<TimeBucketKey, MessageDescriptor> = {
  today: msg`今天`,
  yesterday: msg`昨天`,
  week: msg`本周内`,
  earlier: msg`更早`,
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

/**
 * 一个**条目级动作**对外的全部状态：pending / error 来自调用方的 async-action hook
 * （`useAsyncAction` / `useKeyedAsyncAction`），`run` 已经绑好它作用的那个 id。
 *
 * 这个形状是「编排层持有动作、表现层只渲染」这条分工的接缝：收件箱与传输详情各自把
 * 「哪个方向该调 `pause_send` 还是 `pause_receive`」留在编排层，传下来的只有这三格。
 * 两边曾各自定义一份（一份写 `error: WebError | undefined`、一份写 `error?: WebError`），
 * 同形不同写法，改一处不会牵动另一处——同一个接缝只该有一份定义。
 */
export type ItemAction = {
  pending: boolean;
  error?: WebError;
  run: () => void;
};
