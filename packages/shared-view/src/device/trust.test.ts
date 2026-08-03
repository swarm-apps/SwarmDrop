import { describe, expect, it } from "vitest";
import { TRUST_LEVELS, canSendToDevice, normalizeTrustLevel, policyNoteFor } from "./trust";

describe("normalizeTrustLevel", () => {
  it("defaults an absent level to collaborator", () => {
    expect(normalizeTrustLevel(null)).toBe("collaborator");
    expect(normalizeTrustLevel(undefined)).toBe("collaborator");
  });

  it("passes explicit levels through", () => {
    for (const level of TRUST_LEVELS) {
      expect(normalizeTrustLevel(level)).toBe(level);
    }
  });
});

describe("TRUST_LEVELS", () => {
  // 顺序是产品语义（一条从松到紧的梯度），三端的选择器都按它排。
  it("runs from most to least trusted", () => {
    expect(TRUST_LEVELS).toEqual(["owned", "collaborator", "temporary", "blocked"]);
  });
});

describe("canSendToDevice", () => {
  it("requires the device to be online", () => {
    expect(canSendToDevice({ status: "online", trustLevel: "owned" })).toBe(true);
    expect(canSendToDevice({ status: "offline", trustLevel: "owned" })).toBe(false);
  });

  it("refuses blocked devices even when online", () => {
    expect(canSendToDevice({ status: "online", trustLevel: "blocked" })).toBe(false);
  });

  it("treats an absent trust level as collaborator, which may send", () => {
    expect(canSendToDevice({ status: "online" })).toBe(true);
  });
});

describe("policyNoteFor", () => {
  const auto = { autoAccept: true, requireConfirmation: false };
  const manual = { autoAccept: false, requireConfirmation: true };

  it("reports blocked and temporary from the level alone", () => {
    expect(policyNoteFor("blocked", auto)).toBe("blocked");
    expect(policyNoteFor("temporary", auto)).toBe("temporary");
  });

  it("reports auto acceptance only when confirmation is not required", () => {
    expect(policyNoteFor("owned", auto)).toBe("auto_accept");
    expect(policyNoteFor("owned", { autoAccept: true, requireConfirmation: true })).toBe(
      "manual_confirmation",
    );
    expect(policyNoteFor("collaborator", manual)).toBe("manual_confirmation");
  });
});
