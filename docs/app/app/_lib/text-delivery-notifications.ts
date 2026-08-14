/** 浏览器提醒适配器的最小边界，方便在不具备浏览器 API 的测试中验证决策。 */
export interface TextAttentionNotificationAdapter {
  isSupported(): boolean;
  permission(): NotificationPermission;
  isForeground(): boolean;
  show(notification: {
    title: string;
    body: string;
    tag: string;
    onClick: () => void;
  }): void;
}

/**
 * 仅在节点仍存活、当前标签不在前台且用户已明确授权时创建浏览器通知。
 * 返回值仅说明是否请求了通知；它不代表系统一定成功展示，浏览器 API 没有可靠回执。
 */
export function notifyBackgroundTextDelivery(
  peerName: string,
  title: string,
  body: string,
  adapter: TextAttentionNotificationAdapter = browserTextAttentionNotifier(),
): boolean {
  if (
    !adapter.isSupported() ||
    adapter.permission() !== "granted" ||
    adapter.isForeground()
  ) {
    return false;
  }

  adapter.show({
    title,
    body: `${peerName} · ${body}`,
    tag: `swarmdrop-text-${crypto.randomUUID()}`,
    onClick: openInboxFromNotification,
  });
  return true;
}

function browserTextAttentionNotifier(): TextAttentionNotificationAdapter {
  return {
    isSupported: () =>
      typeof window !== "undefined" && "Notification" in window,
    permission: () =>
      typeof window === "undefined" || !("Notification" in window)
        ? "default"
        : Notification.permission,
    isForeground: () =>
      typeof document !== "undefined" &&
      document.visibilityState === "visible" &&
      document.hasFocus(),
    show: ({ title, body, tag, onClick }) => {
      const notification = new Notification(title, { body, tag });
      notification.onclick = () => {
        onClick();
        notification.close();
      };
    },
  };
}

function openInboxFromNotification(): void {
  if (typeof window === "undefined") return;
  window.focus();
  // 使用当前 app 路径解析，才能保留静态部署时的 Next.js basePath。
  window.location.assign(
    new URL("inbox", new URL("./", window.location.href)).toString(),
  );
}
