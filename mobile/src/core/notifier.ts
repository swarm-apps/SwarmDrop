import * as Notifications from "expo-notifications";

export async function ensureNotificationPermission(): Promise<boolean> {
  const current = await Notifications.getPermissionsAsync();
  if (current.granted) {
    return true;
  }
  const requested = await Notifications.requestPermissionsAsync();
  return requested.granted;
}

export async function notifyTransferOffer(
  deviceName: string,
  fileCount: number,
): Promise<void> {
  const granted = await ensureNotificationPermission();
  if (!granted) {
    return;
  }

  await Notifications.scheduleNotificationAsync({
    content: {
      title: "收到文件传输请求",
      body: `${deviceName} 想发送 ${fileCount} 个文件`,
    },
    trigger: null,
  });
}

export async function notifyPairingRequest(
  peerId: string,
  code: string | undefined,
): Promise<void> {
  const granted = await ensureNotificationPermission();
  if (!granted) {
    return;
  }

  // 配对时还不知道对方 hostname（要等用户接受后才进 device 列表），
  // 用 PeerId 前 10 位 + 配对码做副标题。
  const peerHint = peerId.slice(0, 10);
  const codeHint = code ? `配对码 ${code}` : "扫码配对";
  await Notifications.scheduleNotificationAsync({
    content: {
      title: "新设备请求配对",
      body: `${peerHint}... · ${codeHint}`,
    },
    trigger: null,
  });
}

/**
 * Fire-and-forget 包装：EventBus.emit 来自 Rust 线程，不能 await，
 * 把 notification 投递异常吞掉避免影响事件分发。
 */
export function fireNotifyTransferOffer(
  deviceName: string,
  fileCount: number,
): void {
  notifyTransferOffer(deviceName, fileCount).catch((err) => {
    console.warn("[notifier] notifyTransferOffer failed:", err);
  });
}

export function fireNotifyPairingRequest(
  peerId: string,
  code: string | undefined,
): void {
  notifyPairingRequest(peerId, code).catch((err) => {
    console.warn("[notifier] notifyPairingRequest failed:", err);
  });
}
