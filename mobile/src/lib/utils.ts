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
