import { describe, expect, it } from "vitest";
import { lanHelperAddresses } from "./lan-helper";

const PEER = "12D3KooWMYnFbMsU1dwnPTRcsCHhMHA9MBFxFrCv4puyuiURBaCY";
const RELAY = "12D3KooWCkajTewJhupefZpVK7LwYfjG8bDJyXNtCgQYxiH1utep";
const H1 = "uEiBuBPteUjlXiXM9izTtEdpg3C0QHFZ0A2m6aSjsbv2oeA";
const H2 = "uEiDSOtFQBoepe-LRH2mZPMLHGoMcxnmaM8a02_72my1v9Q";

const WEBTRANSPORT = `/ip4/192.168.1.5/udp/54324/quic-v1/webtransport/certhash/${H1}/certhash/${H2}`;
const WEBRTC_DIRECT = `/ip4/192.168.1.5/udp/54323/webrtc-direct/certhash/${H1}`;

describe("lanHelperAddresses", () => {
  /**
   * 这条是本文件存在的理由：前身用白名单式三元链，WebTransport 上线后被整条静默丢掉，
   * 两端于是都只显示 webrtc-direct —— 用户复制走慢 4.5 倍的那条，且拿不到快的那条。
   */
  it("keeps WebTransport, and puts it first", () => {
    const picked = lanHelperAddresses([WEBRTC_DIRECT, WEBTRANSPORT], PEER);

    expect(picked.map((a) => a.transport)).toEqual([
      "WebTransport",
      "WebRTC Direct",
    ]);
  });

  it("appends the peer id only when missing", () => {
    const withId = `${WEBTRANSPORT}/p2p/${PEER}`;
    const picked = lanHelperAddresses([WEBTRANSPORT, withId], PEER);

    expect(picked[0]?.address).toBe(`${WEBTRANSPORT}/p2p/${PEER}`);
    expect(picked[1]?.address).toBe(withId);
  });

  /** 浏览器拨不了裸 TCP/QUIC，列出来只会让人复制一条注定失败的地址。 */
  it("drops transports the browser cannot dial", () => {
    const picked = lanHelperAddresses(
      [
        "/ip4/192.168.1.5/tcp/54321",
        "/ip4/192.168.1.5/udp/54322/quic-v1",
        WEBRTC_DIRECT,
      ],
      PEER,
    );

    expect(picked.map((a) => a.transport)).toEqual(["WebRTC Direct"]);
  });

  /** 协助地址是为了少一跳；放行 circuit 等于让人复制一条「中继的中继」。 */
  it("drops circuit addresses", () => {
    const circuit = `${WEBRTC_DIRECT}/p2p/${RELAY}/p2p-circuit/p2p/${PEER}`;

    expect(lanHelperAddresses([circuit], PEER)).toEqual([]);
  });

  it("returns nothing without a peer id", () => {
    expect(lanHelperAddresses([WEBTRANSPORT], undefined)).toEqual([]);
  });
});
