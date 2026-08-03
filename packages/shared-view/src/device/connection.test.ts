import { describe, expect, it } from "vitest";
import { transportLabel } from "./connection";

describe("transportLabel", () => {
  it("按内核的 camelCase 判定给出专有名词写法", () => {
    expect(transportLabel("tcp")).toBe("TCP");
    expect(transportLabel("quic")).toBe("QUIC");
    expect(transportLabel("webrtc")).toBe("WebRTC");
    expect(transportLabel("webrtcDirect")).toBe("WebRTC Direct");
  });

  it("地址里读不出 transport 时返回 null，由调用点决定显示什么", () => {
    expect(transportLabel(null)).toBeNull();
    expect(transportLabel(undefined)).toBeNull();
    expect(transportLabel("")).toBeNull();
  });

  it("未知变体原样返回，不吞成「未知」", () => {
    // 后端加了新传输、前端还没跟上时，原始字符串至少还能搜索与比对日志
    expect(transportLabel("webtransport")).toBe("webtransport");
  });
});
