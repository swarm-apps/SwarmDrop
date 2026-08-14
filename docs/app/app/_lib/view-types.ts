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
  // 文件级发布事件（暂存 → 用户可见位置）。**不是会话级**：一个会话收齐一个文件就发布一次，
  // 所以一条 100 文件的会话会发 100 次，散布在整条传输里。
  FilePublishEvent,
  FilePublishPhase,
  PendingPairingJson,
  PairInvitePreviewJson,
  ConnectionJson,
  OfferRejectReason,
  PathKindJson,
  InfraLink,
  RelayLinkState,
  InfraExclusion,
  InfraAddrError,
  BootstrapCandidateSource,
  CandidateScope,
  Device,
  DeviceTrustLevel,
  DeviceReceivePolicy,
  InboxItemDetail,
  InboxItemSummary,
  InboxItemFileEntry,
  InboxSearchHit,
  TextDeliveryRecord,
  PendingTextDeliverySummary,
  InboxHitFile,
  FailureCode,
  ResumeRejectReason,
} from "swarmdrop-web";

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
// 上面那段是**纯再导出**（`export type {...}`），本模块自己用不到那些名字——
// 下面的标签映射要按枚举取值建 Record，所以另 import 一次。
import type {
  InboxItemDetail,
  InboxItemFileEntry,
  InboxItemSummary,
  OfferRejectReason,
  PairingRefusedJson,
  ResumeRejectReason,
  TransferProjection,
  WebError,
} from "swarmdrop-web";
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
  // 这两条对应内核的 `SessionNotFound` / `StorageFailed`（见 crates/web/src/error.rs 的映射）。
  // 写成动作而不是名词：「未找到」「存储错误」说完等于没说。
  notFound: msg`这条记录已经不在了，请返回列表重新开始`,
  storage: msg`保存失败，浏览器存储可能已满或被拒绝`,
};

/**
 * `PairingRefusedJson["type"]` 的标签 —— **对方拒绝配对不是错误**，所以它有自己的文案表，
 * 不走 `WEB_ERROR_KIND_LABEL`。
 *
 * 这里曾经根本不存在：`connect_invite` 把拒绝包成 `WebError::network(中文串)`，于是用户
 * 看到的是标题「网络错误」加一句未经本地化的简体中文，而网络完全正常。桌面
 * （`src/stores/pairing-store.ts`）一直是按 reason 出文案的，Web 这条是遗漏。
 */
export const PAIRING_REFUSED_LABEL: Record<PairingRefusedJson["type"], MessageDescriptor> = {
  user_rejected: msg`对方拒绝了这次配对`,
};

/**
 * 会话失败判别码 → 可翻译描述符。
 *
 * 与 `WEB_ERROR_KIND_LABEL` 是**两条独立通道**：那条是命令返回值（`WebError.kind`），
 * 这条是 `TransferProjection.failure`（本地会话状态，不进 wire）。两者共用同一条契约 ——
 * 内核回答「是什么失败」，措辞归三端各自的 catalog。
 *
 * 这里曾经是**裸渲染** `projection.errorMessage`：一句 Rust 侧拼好的中文，英文界面上照样弹。
 *
 * 返回描述符而不是字符串，同 `WEB_ERROR_KIND_LABEL` 的理由：本模块是纯类型/常量层，
 * 翻译宏只能定义、展开是组件的事。带参数的那几条由 `msg` 把值一起烤进描述符。
 */
export function failureCodeLabel(
  failure: TransferProjection["failure"],
): MessageDescriptor | null {
  if (!failure) return null;
  switch (failure.code) {
    case "sessionExpired":
      return msg`超过 ${failure.retentionDays} 天未恢复，已自动清理`;
    case "resumeRejected":
      return RESUME_REJECT_LABEL[failure.reason.type];
    case "offerFailed":
      return msg`发送请求没能送达对方，请确认对方在线后重试`;
    case "peerProtocolUnsupported":
      return msg`对方的 SwarmDrop 版本太旧，无法接收这次传输，请让对方升级后重试`;
    case "legacy":
      // 判别码引入之前落库的自由文本，原样展示（多为简体中文）。
      return { id: failure.message, message: failure.message };
  }
}

/** 续传被对端拒绝的原因 → 描述符。 */
const RESUME_REJECT_LABEL: Record<ResumeRejectReason["type"], MessageDescriptor> = {
  cancelled: msg`对方已取消这次传输，无法继续`,
  fatal_error: msg`对方那边出错了，无法继续`,
  source_modified: msg`源文件已变更，无法继续，请重新发送`,
  checkpoint_invalid: msg`续传进度已失效，请重新发送`,
  peer_unavailable: msg`对方暂时不可用，请稍后再试`,
  session_not_found: msg`对方已经没有这次传输的记录了`,
};

/**
 * 收件箱条目标题：`title` 是**首个文件名**，「等 N 个文件」这句由这里生成。
 *
 * 曾经是内核渲染好整句再落库（`inbox_title`），于是历史条目的标题永远冻结在写入时的
 * 语言。库里现在只存与语言无关的文件名。
 */
export function inboxItemTitleLabel(title: string, itemCount: number): MessageDescriptor {
  if (itemCount === 0) return msg`空传输`;
  if (itemCount === 1) return { id: title, message: title };
  return msg`${title} 等 ${itemCount} 个文件`;
}

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
 * 收件箱下载的 keyed-action 键 —— 三种目标共用一个 `useKeyedAsyncAction`，靠形态区分。
 *
 * | 目标 | 键 | 会不会与别的撞 |
 * |---|---|---|
 * | 某个文件 | `${itemId}:${fileId}` | `fileId` 是自增主键，`toString()` 后恒为数字串 |
 * | 某个目录 | `${itemId}:dir:${relativePath}` | `dir:` 前缀既不是数字串也不是 `all` |
 * | 整条记录 | `${itemId}:all` | 同上 |
 *
 * 三者独立可查，于是「批量在不在跑」与「某一行在不在跑」不会互相冒充——用逐文件 pending
 * 的并集去判批量，单点一行也会把头部按钮变成「下载中…」。
 *
 * 住在 `_lib/` 而不是任一组件里：写键的是 `receive-panel`（编排），读键的是 `inbox-views`
 * （呈现），而前者已经 import 后者——放在任一端都会成环。
 *
 * **两个后缀常量不导出**：外部只该经下面四个函数编解码，拿到裸字符串自己拼就等于把
 * 这套编码复制了一份。
 */
const DOWNLOAD_ALL_KEY = "all";
const DOWNLOAD_DIR_PREFIX = "dir:";

export const fileDownloadKey = (itemId: string, fileId: number) =>
  `${itemId}:${fileId}`;

export const directoryDownloadKey = (itemId: string, relativePath: string) =>
  `${itemId}:${DOWNLOAD_DIR_PREFIX}${relativePath}`;

export const allDownloadKey = (itemId: string) =>
  `${itemId}:${DOWNLOAD_ALL_KEY}`;

/**
 * 一条收件箱记录里**还取得回来**的文件。
 *
 * 「全部下载」「目录下载」「发送到设备」共用这一条谓词——三者的判据本就相同，而
 * `missing` 将来会由 OPFS 驱逐检测真正置起来（今天恒 `false`，见调用点的说明）。
 * 散在各处的话，那一天要改三个地方，而漏掉的那处会悄悄把取不回的文件打进压缩包。
 */
export function usableInboxFiles(
  item: InboxItemDetail,
): InboxItemFileEntry[] {
  return inboxFiles(item).filter((file) => !file.missing);
}

/** 收件箱详情是联合模型；文件操作只能读取 Files 分支。 */
export function inboxFiles(item: InboxItemDetail): InboxItemFileEntry[] {
  return item.content.kind === "files" ? item.content.entries : [];
}

/** 传输投影只属于文件收件箱，文本投递没有会话记录可跳转。 */
export function inboxTransfer(item: InboxItemDetail): TransferProjection | null {
  return item.content.kind === "files" ? item.content.transfer : null;
}

export function inboxTextBody(item: InboxItemDetail): string | null {
  return item.content.kind === "text" ? item.content.body : null;
}

/** 一把下载键指向什么。`parseDownloadKey` 的产物。 */
export type DownloadTarget =
  | { kind: "all" }
  | { kind: "directory"; relativePath: string }
  | { kind: "file"; fileId: number };

/**
 * 下载键 → 它作用在什么上；**不属于本条目**的键给 `null`。
 *
 * 后半句不是防御性判断：一个 `useKeyedAsyncAction` 实例服务收件箱里的所有条目，
 * 键集合是全局的，读的人必须自己筛——而按前缀筛是 O(1)，不必先把 `item.files` 扫一遍
 * 才发现这条根本不是自己的。
 *
 * 它与上面四个构造函数成对。**解码集中在这一处**，调用方就不必各自认识 `dir:` / `all`
 * 这些字面量：pending 与失败提示两条路径都问同一个函数「这是谁」。
 */
export function parseDownloadKey(
  itemId: string,
  key: string,
): DownloadTarget | null {
  const prefix = `${itemId}:`;
  if (!key.startsWith(prefix)) return null;
  const rest = key.slice(prefix.length);
  if (rest === DOWNLOAD_ALL_KEY) return { kind: "all" };
  if (rest.startsWith(DOWNLOAD_DIR_PREFIX)) {
    return { kind: "directory", relativePath: rest.slice(DOWNLOAD_DIR_PREFIX.length) };
  }
  // **只认纯数字串**，不用 `Number.isInteger(Number(rest))`：`Number("")` 是 0，于是
  // 任何形如 `${itemId}:` 的键都会被认成「0 号文件」；`"1e3"` 与 `" 7 "` 同样会蒙混过关。
  return /^\d+$/.test(rest) ? { kind: "file", fileId: Number(rest) } : null;
}

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
