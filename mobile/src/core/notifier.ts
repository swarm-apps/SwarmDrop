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
