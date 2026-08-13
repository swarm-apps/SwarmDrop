import { describe, expect, it } from "vitest";
import { peerLabel } from "./device-presentation";

const PEER = "12D3KooWP4M1b3qc6tb147qkbEm8fYa7JpcLpGJW4WE2dpK1mJoL";

describe("peerLabel", () => {
  it("有名字就用名字", () => {
    expect(peerLabel("MacBook Pro", PEER)).toBe("MacBook Pro");
  });

  it("名字为空时回落短 PeerId，而不是留白", () => {
    expect(peerLabel("", PEER)).toBe("12D3…K1mJoL");
    expect(peerLabel("   ", PEER)).toBe("12D3…K1mJoL");
  });

  // 跨 wasm 边界的形状类型层保证不了（知识库那条 `.d.ts` 说 string、运行时是别的）。
  // 五个渲染点共用这一个函数，任何一处 `undefined.trim()` 都会把整页打白。
  it("名字缺失也不抛", () => {
    expect(peerLabel(null, PEER)).toBe("12D3…K1mJoL");
    expect(peerLabel(undefined, PEER)).toBe("12D3…K1mJoL");
  });
});
