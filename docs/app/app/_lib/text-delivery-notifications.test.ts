import { describe, expect, it } from "vitest";
import {
  notifyBackgroundTextDelivery,
  type TextAttentionNotificationAdapter,
} from "./text-delivery-notifications";

function fakeAdapter(overrides: Partial<TextAttentionNotificationAdapter> = {}) {
  const shown: Parameters<TextAttentionNotificationAdapter["show"]>[0][] = [];
  const adapter: TextAttentionNotificationAdapter = {
    isSupported: () => true,
    permission: () => "granted",
    isForeground: () => false,
    show: (notification) => shown.push(notification),
    ...overrides,
  };
  return { adapter, shown };
}

describe("notifyBackgroundTextDelivery", () => {
  it("只在已授权的后台页发出不含正文的通知", () => {
    const { adapter, shown } = fakeAdapter();

    expect(notifyBackgroundTextDelivery("Alice", "收到文本", "等待你的确认", adapter)).toBe(true);
    expect(shown).toHaveLength(1);
    expect(shown[0]).toMatchObject({
      title: "收到文本",
      body: "Alice · 等待你的确认",
    });
    expect(shown[0].body).not.toContain("敏感正文");
  });

  it.each([
    ["未授权", fakeAdapter({ permission: () => "denied" })],
    ["API 不可用", fakeAdapter({ isSupported: () => false })],
    ["前台页面", fakeAdapter({ isForeground: () => true })],
  ])("在%s时只保留应用内反馈", (_reason, fixture) => {
    expect(notifyBackgroundTextDelivery("Alice", "收到文本", "等待你的确认", fixture.adapter)).toBe(false);
    expect(fixture.shown).toHaveLength(0);
  });
});
