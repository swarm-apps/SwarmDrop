import { describe, expect, it } from "vitest";
import { validateSendSearch } from "./send";
import { validateShareTargetSearch } from "./send/share-target";
import { validateTransferSearch } from "./transfer";

describe("route-owned search params", () => {
  it("keeps transfer detail and non-default filter in the route", () => {
    expect(
      validateTransferSearch({
        session: "session-1",
        filter: "recoverable",
      }),
    ).toEqual({ session: "session-1", filter: "recoverable" });
  });

  it("drops default or invalid transfer filter values", () => {
    expect(validateTransferSearch({ filter: "all" })).toEqual({});
    expect(validateTransferSearch({ filter: "unknown" })).toEqual({});
  });

  it("keeps send progress session in the route", () => {
    expect(
      validateSendSearch({
        peerId: "peer-1",
        session: "session-2",
      }),
    ).toEqual({ peerId: "peer-1", session: "session-2" });
  });

  it("keeps share-target progress session in the route", () => {
    expect(validateShareTargetSearch({ session: "session-3" })).toEqual({
      session: "session-3",
    });
    expect(validateShareTargetSearch({ session: "" })).toEqual({});
  });
});
