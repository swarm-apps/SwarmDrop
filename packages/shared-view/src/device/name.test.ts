import { describe, expect, it } from "vitest";
import { DEVICE_NAME_MAX_CHARS, deviceDisplayName } from "./name";

describe("deviceDisplayName", () => {
  it("prefers the user-set name", () => {
    expect(deviceDisplayName({ name: "办公室 Mac", hostname: "macbook-pro" })).toBe("办公室 Mac");
  });

  it("falls back to hostname when the name is absent, empty, or blank", () => {
    expect(deviceDisplayName({ hostname: "macbook-pro" })).toBe("macbook-pro");
    expect(deviceDisplayName({ name: null, hostname: "macbook-pro" })).toBe("macbook-pro");
    expect(deviceDisplayName({ name: "", hostname: "macbook-pro" })).toBe("macbook-pro");
    expect(deviceDisplayName({ name: "   ", hostname: "macbook-pro" })).toBe("macbook-pro");
  });

  it("trims the name it returns", () => {
    expect(deviceDisplayName({ name: "  办公室 Mac  ", hostname: "macbook-pro" })).toBe("办公室 Mac");
  });
});

describe("DEVICE_NAME_MAX_CHARS", () => {
  // 事实源是 Rust 的 `DeviceName::MAX_CHARS`（crates/host/src/device.rs）。这条测试不验证
  // 「40 是对的」——它验证的是「三端读到的是同一个 40」，改动时至少要有人把这里一起改。
  it("mirrors the Rust-side limit", () => {
    expect(DEVICE_NAME_MAX_CHARS).toBe(40);
  });
});
