import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function errorMessage(err: unknown): string {
  if (!(err instanceof Error)) return String(err);
  // ubrn 的 UniffiError 只把 "EnumName.Variant" 塞进 message,真正的内容
  // 在 inner 数组里(uniffi enum variant 的关联字段)。展开后才是人看的内容。
  // 例:FfiError.Transfer { inner: ["session already finished"] }
  //    → "FfiError.Transfer: session already finished"
  const inner = (err as unknown as { inner?: unknown }).inner;
  if (Array.isArray(inner) && inner.length > 0) {
    return `${err.message}: ${inner.map((v) => String(v)).join(", ")}`;
  }
  return err.message;
}

export function truncateMiddle(value: string, head = 8, tail = 4): string {
  if (value.length <= head + tail + 2) return value;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}

/**
 * file:// / content:// URI 的显示用尾段:decode 后取最后一段路径(目录/文件名)。
 * 畸形 URI(decode 抛错)原样返回。回退文案(「收件箱」「默认位置」等)由调用方处理。
 */
export function lastPathSegment(uri: string): string {
  try {
    const decoded = decodeURIComponent(uri.replace(/\/+$/, ""));
    const segments = decoded.split("/");
    const last = segments[segments.length - 1];
    return last && last.length > 0 ? last : decoded;
  } catch {
    return uri;
  }
}

/** 去掉路径最后一段,返回父目录部分;无目录层级时返回 ""。对 URI 与相对路径都适用。 */
export function parentDirOf(path: string): string {
  return path.replace(/\/+$/, "").split("/").slice(0, -1).join("/");
}
