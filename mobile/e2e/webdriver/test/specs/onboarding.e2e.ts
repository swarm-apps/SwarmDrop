import { browser, expect } from "@wdio/globals";
import {
  completeOnboardingIfNeeded,
} from "../helpers/ios";

describe("SwarmDrop iOS WebDriver smoke", () => {
  it("completes onboarding or lands on the main shell", async () => {
    const shell = await completeOnboardingIfNeeded();
    expect(shell).toBeTruthy();
    await browser.pause(250);
  });
});
