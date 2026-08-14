import { describe, expect, it } from "vitest";

import {
  TEXT_DELIVERY_MAX_BYTES,
  formatTextDeliveryKiB,
  isTextDeliveryRetryable,
  isTextDeliveryWithinLimit,
  textDeliveryNotice,
  textDeliveryStatusKey,
  utf8ByteLength,
} from "./text-delivery";

describe("text delivery view semantics", () => {
  it("counts UTF-8 bytes instead of UTF-16 characters", () => {
    expect(utf8ByteLength("A中文😀")).toBe(11);
    expect(isTextDeliveryWithinLimit("😀".repeat(16_384))).toBe(true);
    expect(isTextDeliveryWithinLimit("😀".repeat(16_385))).toBe(false);
  });

  it("keeps the boundary and KiB display deterministic", () => {
    expect(isTextDeliveryWithinLimit("a".repeat(TEXT_DELIVERY_MAX_BYTES))).toBe(true);
    expect(isTextDeliveryWithinLimit("a".repeat(TEXT_DELIVERY_MAX_BYTES + 1))).toBe(false);
    expect(formatTextDeliveryKiB(1024)).toBe("1 KiB");
    expect(formatTextDeliveryKiB(1536)).toBe("1.5 KiB");
  });

  it("keeps retry and status semantics distinct", () => {
    expect(isTextDeliveryRetryable("retryable")).toBe(true);
    expect(isTextDeliveryRetryable("delivered")).toBe(false);
    expect(textDeliveryStatusKey("waiting_confirmation")).toBe(
      "textDelivery.status.waitingConfirmation",
    );
  });

  it("builds notifications without a text body", () => {
    expect(textDeliveryNotice("Alice")).toEqual({
      titleKey: "textDelivery.notice.title",
      bodyKey: "textDelivery.notice.body",
      values: { deviceName: "Alice" },
    });
  });
});
