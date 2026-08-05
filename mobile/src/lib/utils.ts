import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
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
