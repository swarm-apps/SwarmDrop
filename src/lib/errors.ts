/**
 * 后端错误处理工具
 *
 * Tauri invoke 失败时抛出的是后端 AppError 序列化后的对象：
 * `{ kind: "NodeNotStarted", message: "Node not started" }`
 *
 * 本地化原则：后端 `kind` 是稳定、语言无关的判别码，用户可读文案由前端按 `kind`
 * 经 Lingui 生成；后端 `message` 仅作开发者/日志用技术细节，不直接展示给用户。
 */

import { i18n } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";

/** 后端 AppError 序列化格式 */
export interface AppError {
  kind: string;
  message: string;
}

/** 判断错误是否为后端 AppError */
export function isAppError(err: unknown): err is AppError {
  return (
    typeof err === "object" &&
    err !== null &&
    "kind" in err &&
    "message" in err
  );
}

/** 判断错误是否为特定 kind */
export function isErrorKind(err: unknown, kind: string): boolean {
  return isAppError(err) && err.kind === kind;
}

/**
 * 有明确用户语义的错误 kind → 本地化消息描述符。
 * 携带自由文本细节的 kind（Network / Transfer / Identity）只本地化 kind 级标题，
 * 内嵌技术细节仍保留在 err.message 供日志/详情，不进 toast。
 */
const KIND_MESSAGES: Record<string, MessageDescriptor> = {
  NodeNotStarted: msg`节点未启动`,
  ExpiredCode: msg`配对码已过期`,
  InvalidCode: msg`无效的配对码`,
  Network: msg`网络连接出现问题，请稍后重试`,
  Transfer: msg`文件传输失败，请重试`,
  Identity: msg`设备身份初始化失败`,
};

/** 通用兜底提示（技术细节仅留在 err.message 供日志/详情）。 */
const GENERIC_ERROR = msg`出错了，请重试`;

/**
 * 从错误中提取当前语言下的用户可读消息。
 *
 * - 命中语义映射（`KIND_MESSAGES`）→ 返回当前 locale 文案
 * - 未命中 → 通用「出错了，请重试」。这涵盖内部/技术类 kind（`Io` / `Serialization` /
 *   `Database` / `TaskJoin` / `P2p` / `Tauri`）：不逐条翻译，也不把后端英文原文丢给用户；
 *   原始 message 仍在 err 上供日志/详情。
 * - 非 AppError → 原生 Error.message / String(err)
 */
export function getErrorMessage(err: unknown): string {
  if (isAppError(err)) {
    const descriptor = KIND_MESSAGES[err.kind];
    return descriptor ? i18n._(descriptor) : i18n._(GENERIC_ERROR);
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
