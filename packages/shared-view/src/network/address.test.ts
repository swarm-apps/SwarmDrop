import { describe, expect, it } from "vitest";

import { transportFromAddr, truncateAddr } from "./index";

/**
 * `transportFromAddr` 是**顺序敏感**的 if 链：一条 multiaddr 可以同时含多个传输段，
 * 先匹配到哪个就标成哪个。
 *
 * 这类判据没有编译期保护 —— 加一个新传输时把它插错位置，表现是「标签显示成另一种传输」，
 * 而校验那边可能同时以 `unsupportedTransport` 拒掉它，同一屏上两句话互相矛盾。
 * 本文件是那条顺序的唯一看守。
 */
describe("transportFromAddr", () => {
  it("按特异性从高到低匹配复合地址", () => {
    // 每条都**同时含**两个及以上传输段，标签必须取最具体的那个。
    const cases: [string, string][] = [
      // /webrtc-direct 含 /webrtc
      [
        "/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/uEiBu/p2p/12D3KooW",
        "WebRTC Direct",
      ],
      // /webtransport 地址必然含 /quic-v1
      [
        "/ip4/47.115.172.218/udp/4004/quic-v1/webtransport/certhash/uEiBu/p2p/12D3KooW",
        "WebTransport",
      ],
      // CA 签名证书那条路径没有 /certhash，同样要认出来
      ["/dns4/relay.example.com/udp/443/quic-v1/webtransport", "WebTransport"],
      // /wss 含 /ws
      ["/dns4/relay.example.com/tcp/443/wss/p2p/12D3KooW", "WSS"],
    ];

    for (const [addr, expected] of cases) {
      expect(transportFromAddr(addr), addr).toBe(expected);
    }
  });

  it("单一传输段原样识别", () => {
    expect(transportFromAddr("/ip4/1.2.3.4/udp/4001/quic-v1")).toBe("QUIC");
    expect(transportFromAddr("/ip4/1.2.3.4/tcp/4001")).toBe("TCP");
    expect(transportFromAddr("/dns4/x.example.com/tcp/80/ws")).toBe(
      "WebSocket",
    );
  });

  it("认不出来时回落到中性词，而不是空字符串", () => {
    expect(transportFromAddr("/ip4/1.2.3.4")).toBe("P2P");
    expect(transportFromAddr("")).toBe("P2P");
  });
});

describe("truncateAddr", () => {
  it("短地址原样返回", () => {
    const short = "/ip4/1.2.3.4/tcp/4001";
    expect(truncateAddr(short)).toBe(short);
  });

  /**
   * **前缀被截时必须留 `…`。** 桌面那版没有，输出看起来像一条完整但写错的 multiaddr
   * （certhash 整段消失、切口无标记），而用户会照着它去 issue 里贴。
   */
  it("截断前缀时留下省略号", () => {
    const long =
      "/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA/p2p/12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep";
    const out = truncateAddr(long);

    expect(out.length).toBeLessThan(long.length);
    expect(out).toContain("…");
    // 头尾都要能认出来：协议头用于判断传输，末尾几位用于比对 peer id。
    expect(out.startsWith("/ip4/47.115.172.218")).toBe(true);
    expect(out).toContain("1utep");
  });

  /**
   * **护栏：circuit 地址取的是末位那个 `/p2p/`。**
   *
   * 这是本次三份合并唯一真正改掉的判据。用 `indexOf` 的话会命中中转身份，`/p2p-circuit/`
   * 整段被静默吃掉 —— 一条中继地址被渲染成直连地址，那串"peer id"是 relay 头 6 位 +
   * target 尾 5 位的拼接，不属于任何一方。有人"简化"回 `indexOf` 时这条必须变红。
   */
  it("circuit 地址展示的是目标身份，不是中转身份", () => {
    const relay = "12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep";
    const target = "12D3KooWBimNCjkNXz2YuwGELEThz7NcTqgN53tW62mcKzKhCcLu";
    const circuit = `/ip4/47.115.172.218/udp/4001/quic-v1/p2p/${relay}/p2p-circuit/p2p/${target}`;

    const out = truncateAddr(circuit);

    // `indexOf` 版会把 peer 段算成 `<relay>/p2p-circuit/p2p/<target>`，于是展示出
    // relay 的头 6 位 + target 的尾 5 位 —— 一个不属于任何一方的"peer id"。
    expect(out).toContain(target.slice(-5));
    expect(out).not.toContain(relay.slice(-5));
    // 前缀确实被截了（60 字符装不下完整的 circuit 头），但切口有 `…` 标记。
    expect(out).toContain("…");
  });

  it("没有 /p2p/ 段时也能截", () => {
    const long = `/ip4/1.2.3.4/udp/4001/quic-v1/${"x".repeat(80)}`;
    expect(truncateAddr(long)).toContain("…");
  });
});
