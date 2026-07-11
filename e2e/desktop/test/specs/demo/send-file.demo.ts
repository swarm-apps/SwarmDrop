import { $, $$, expect } from "@wdio/globals";
import {
  captureDemoFrame,
  pauseForRecording,
  waitForDesktopHome,
} from "./helpers";

async function firstEnabledSendAction() {
  const actions = await $$('[data-testid="device-send-action"]');
  for (const action of actions) {
    if (await action.isEnabled()) return action;
  }
  return null;
}

describe("SwarmDrop desktop send-file demo", () => {
  it("records the send entry and file selection surface", async function () {
    this.timeout(180_000);

    await waitForDesktopHome();
    await pauseForRecording(250);

    const sendAction = await firstEnabledSendAction();
    if (!sendAction) {
      await captureDemoFrame("send-file-no-online-device");
      await pauseForRecording(500);
      return;
    }

    await sendAction.scrollIntoView();
    await pauseForRecording(250);
    await sendAction.click();

    await expect($('[data-testid="send-page"]')).toBeDisplayed({ wait: 10_000 });
    await expect($('[data-testid="send-target-summary"]')).toBeDisplayed();
    await expect($('[data-testid="file-drop-zone"]')).toBeDisplayed();
    await expect($('[data-testid="send-empty-selection"]')).toBeDisplayed();

    await pauseForRecording(300);
    await captureDemoFrame("send-file-selection");
    await pauseForRecording(500);
  });
});
