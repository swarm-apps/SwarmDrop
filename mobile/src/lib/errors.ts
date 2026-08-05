/**
 * 后端错误处理工具（桌面 `src/lib/errors.ts` 的移动端对应物）。
 *
 * uniffi 抛上来的是 `FfiError` 的某个变体，形状是 `{ tag, inner }`：`tag` 是与 Rust
 * `AppError` 一一对应的**稳定判别码**，`inner` 里的 message 是 Rust 侧写的技术描述。
 *
 * **`message` 不能直接展示。** 它是中文串（`"收件箱条目不存在"`、`"file sink not found: …"`），
 * 英文界面下会原样露出来。桌面与 Web 一直是按 kind 出本地化文案的，移动端此前**没有这张表**：
 * 20 处 `toast.error(…, errorMessage(err))` 弹的是 `FfiError.Transfer: 收件箱条目不存在`，
 * 另有 6 处 `err instanceof Error ? err.message : String(err)` 把它塞进页面 state。
 *
 * 判据与另两端一致：**有明确用户动作的 kind 才单列**，其余走通用兜底，技术细节只进
 * `console`。见 `crates/host/src/error.rs` 里 `AppError::Transfer` 的文档。
 */

import type { MessageDescriptor } from "@lingui/core";
import { i18n } from "@lingui/core";
import { msg } from "@lingui/core/macro";

/**
 * uniffi 错误的最小形状。
 *
 * 不 import `FfiError` 本体做 `instanceOf` 判别：那需要逐个变体枚举（`FfiError.Io.instanceOf(e)
 * || FfiError.Network.instanceOf(e) || …`），每加一个变体就要改这里，而我们只需要 `tag`。
 */
interface FfiErrorLike {
  tag: string;
}

/** 是否为后端抛上来的 `FfiError`。 */
export function isFfiError(err: unknown): err is FfiErrorLike {
  return (
    typeof err === "object" &&
    err !== null &&
    "tag" in err &&
    typeof (err as { tag: unknown }).tag === "string"
  );
}

/** 是否为特定 kind 的后端错误。 */
export function isErrorKind(err: unknown, tag: string): boolean {
  return isFfiError(err) && err.tag === tag;
}

/**
 * 有明确用户语义的 kind → 本地化消息描述符。
 *
 * 键必须与 `FfiError_Tags`（进而与 Rust `AppError` 的变体名）逐字一致。
 */
const KIND_MESSAGES: Record<string, MessageDescriptor> = {
  NodeNotStarted: msg`节点未启动`,
  // 判别码名字是 6 位配对码时代的历史，语义早已是 PairInvite（见 crates/host 的注释）。
  ExpiredCode: msg`邀请已过期`,
  InvalidCode: msg`邀请无效或已被使用`,
  Network: msg`网络连接出现问题，请稍后重试`,
  Transfer: msg`文件传输失败，请重试`,
  SessionNotFound: msg`这条记录已经不在了，请返回列表重新开始`,
  StorageFailed: msg`保存失败，请检查存储空间后重试`,
  Identity: msg`设备身份读写失败`,
  IdentityNotReady: msg`设备身份尚未就绪，请重启应用后重试`,
  // 邀请凭证是好的，是本机没能把「已消费」写进库。宁可让这次配对失败也不放行——
  // 否则重启后同一份一次性凭证还能再被用一次。
  InvitePersistFailed: msg`邀请状态未能保存，本次配对已中止，请重新生成邀请`,
  DeviceNotFound: msg`未找到该设备`,
  // InvalidArgument 刻意不列：入参解析失败对用户没有意义（要么是 UI 传错了值，
  // 要么是内部标识格式不对），走通用兜底，技术细节留给 console。
};

/** 通用兜底提示。 */
const GENERIC_ERROR = msg`出错了，请重试`;

/**
 * 后端错误的技术细节，**只给 console / 日志**。
 *
 * ubrn 的 `UniffiError` 只把 `"FfiError.Variant"` 塞进 `message`，真正的内容在 `inner`
 * 数组里（uniffi enum variant 的关联字段），展开后才有诊断价值。
 *
 * 这段逻辑原本叫 `errorMessage`，住在 `lib/utils.ts`，被 20 处 `toast.error` 当**用户文案**
 * 用 —— 于是英文界面上会弹出 `FfiError.Transfer: 收件箱条目不存在`。它现在改名进了这里，
 * 名字里的 `detail` 就是那道边界。
 */
export function errorDetail(err: unknown): string {
  if (!(err instanceof Error)) return String(err);
  const inner = (err as unknown as { inner?: unknown }).inner;
  if (Array.isArray(inner) && inner.length > 0) {
    return `${err.message}: ${inner.map((v) => String(v)).join(", ")}`;
  }
  return err.message;
}

/**
 * 从错误中提取当前语言下的用户可读消息。
 *
 * - 命中语义映射 → 当前 locale 文案
 * - 未命中的 `FfiError`（`Io` / `Serialization` / `Database` / `InvalidArgument`）→ 通用兜底。
 *   **不逐条翻译，也不把 Rust 原文丢给用户**。
 * - 非 `FfiError`（JS 运行时异常）→ 原生 `Error.message`。这类串由 JS 侧产生，
 *   不存在「中文串进英文界面」的问题。
 *
 * **它会把技术细节写进 console。** 一个格式化函数带副作用不常见，这里是刻意的：
 * 后端 `message` 从此不进 UI，若不在这里落一份，20 个调用点里没有一个会记得自己去 log，
 * 排查时就只剩一句「出错了，请重试」。本函数只在 catch 分支被调用，不在渲染路径上。
 */
export function getErrorMessage(err: unknown): string {
  if (isFfiError(err)) {
    console.warn("[error]", errorDetail(err));
    const descriptor = KIND_MESSAGES[err.tag];
    return descriptor ? i18n._(descriptor) : i18n._(GENERIC_ERROR);
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
