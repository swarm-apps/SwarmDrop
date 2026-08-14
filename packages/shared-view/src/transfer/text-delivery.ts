/** 三端文本投递共用的纯视图语义，不绑定任何宿主的 i18n 或渲染实现。 */

export const TEXT_DELIVERY_MAX_BYTES = 64 * 1024;

export const TEXT_DELIVERY_MODES = ["files", "text"] as const;
export type TextDeliveryMode = (typeof TEXT_DELIVERY_MODES)[number];

export type TextDeliveryStatus =
  | "sending"
  | "waiting_confirmation"
  | "delivered"
  | "rejected"
  | "retryable"
  | "expired"
  | "cancelled";

export type TextDeliveryStatusKey =
  | "textDelivery.status.sending"
  | "textDelivery.status.waitingConfirmation"
  | "textDelivery.status.delivered"
  | "textDelivery.status.rejected"
  | "textDelivery.status.retryable"
  | "textDelivery.status.expired"
  | "textDelivery.status.cancelled";

export interface TextDeliveryConfirmationItem {
  deliveryId: string;
  peerId: string;
  peerName: string;
  body: string;
  createdAt: number;
}

export interface TextDeliveryInboxLocation {
  textDeliveryId: string;
}

export interface TextDeliveryNotice {
  titleKey: "textDelivery.notice.title";
  bodyKey: "textDelivery.notice.body";
  values: { deviceName: string };
}

/**
 * 按 UTF-8 而非 UTF-16 code unit 计量；三端输入法都可能产生代理对，不能用 `length` 冒充字节数。
 */
export function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code < 0x80) {
      bytes += 1;
    } else if (code < 0x800) {
      bytes += 2;
    } else if (code >= 0xd800 && code <= 0xdbff && index + 1 < value.length) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        bytes += 4;
        index += 1;
      } else {
        bytes += 3;
      }
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

export function isTextDeliveryWithinLimit(value: string): boolean {
  return value.length > 0 && utf8ByteLength(value) <= TEXT_DELIVERY_MAX_BYTES;
}

export function formatTextDeliveryKiB(bytes: number): string {
  const kib = Math.max(0, bytes) / 1024;
  return `${Number.isInteger(kib) ? kib : kib.toFixed(1)} KiB`;
}

export function textDeliveryStatusKey(status: TextDeliveryStatus): TextDeliveryStatusKey {
  switch (status) {
    case "sending":
      return "textDelivery.status.sending";
    case "waiting_confirmation":
      return "textDelivery.status.waitingConfirmation";
    case "delivered":
      return "textDelivery.status.delivered";
    case "rejected":
      return "textDelivery.status.rejected";
    case "retryable":
      return "textDelivery.status.retryable";
    case "expired":
      return "textDelivery.status.expired";
    case "cancelled":
      return "textDelivery.status.cancelled";
  }
}

export function isTextDeliveryRetryable(status: TextDeliveryStatus): boolean {
  return (
    status === "retryable" ||
    status === "expired" ||
    status === "waiting_confirmation"
  );
}

export function textDeliveryInboxLocation(deliveryId: string): TextDeliveryInboxLocation {
  return { textDeliveryId: deliveryId };
}

export function textDeliveryNotice(deviceName: string): TextDeliveryNotice {
  return {
    titleKey: "textDelivery.notice.title",
    bodyKey: "textDelivery.notice.body",
    values: { deviceName },
  };
}
