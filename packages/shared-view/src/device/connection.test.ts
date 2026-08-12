import { describe, expect, it } from "vitest";
import { transportLabel } from "./connection";

describe("transportLabel", () => {
  it("按内核的 camelCase 判定给出专有名词写法", () => {
    expect(transportLabel("tcp")).toBe("TCP");
    expect(transportLabel("quic")).toBe("QUIC");
    expect(transportLabel("webrtc")).toBe("WebRTC");
    expect(transportLabel("webrtcDirect")).toBe("WebRTC Direct");
    expect(transportLabel("webtransport")).toBe("WebTransport");
  });

  it("地址里读不出 transport 时返回 null，由调用点决定显示什么", () => {
    expect(transportLabel(null)).toBeNull();
    expect(transportLabel(undefined)).toBeNull();
    expect(transportLabel("")).toBeNull();
  });

  it("未知变体原样返回，不吞成「未知」", () => {
    // 后端加了新传输、前端还没跟上时，原始字符串至少还能搜索与比对日志。
    //
    // ⚠️ 这里刻意用一个**不打算实现**的名字。上一版拿 `webtransport` 当例子，
    // 而它后来真的实现了 —— 于是这条测试从「验证兜底行为」变成「阻止新变体被加进表里」，
    // 加的人只会看到一条莫名其妙的红。**反例要选真反例。**
    expect(transportLabel("carrier-pigeon")).toBe("carrier-pigeon");
  });
});
