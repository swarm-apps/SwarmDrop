"use client";

import { Trans } from "@lingui/react/macro";
import { Bell } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { SettingsCard, SettingsRow, SettingsSection } from "./settings-primitives";

type PermissionState = "checking" | "unsupported" | NotificationPermission;

/** 浏览器权限只能由用户手势申请；事件到达时绝不突兀地弹权限框。 */
export function TextNotificationPanel() {
  const [permission, setPermission] = useState<PermissionState>("checking");

  useEffect(() => {
    setPermission("Notification" in window ? Notification.permission : "unsupported");
  }, []);

  const enable = async () => {
    if (!("Notification" in window)) return;
    setPermission(await Notification.requestPermission());
  };

  const description =
    permission === "granted" ? (
      <Trans>浏览器在后台时，新的文本会以系统通知提醒你。</Trans>
    ) : permission === "denied" ? (
      <Trans>浏览器已阻止提醒；请在站点权限中重新允许通知。</Trans>
    ) : permission === "unsupported" ? (
      <Trans>当前浏览器不支持系统通知。</Trans>
    ) : (
      <Trans>仅在你允许后，浏览器在后台时才会显示文本提醒。</Trans>
    );

  return (
    <SettingsSection icon={Bell} title={<Trans>提醒</Trans>}>
      <SettingsCard>
        <SettingsRow
          title={<Trans>后台文本提醒</Trans>}
          description={description}
          action={
            <Button
              type="button"
              size="sm"
              className="min-h-11 sm:min-h-9"
              disabled={permission === "checking" || permission === "unsupported" || permission === "denied" || permission === "granted"}
              onClick={() => void enable()}
            >
              {permission === "granted"
                ? <Trans>已开启</Trans>
                : permission === "denied"
                  ? <Trans>已被阻止</Trans>
                  : <Trans>允许通知</Trans>}
            </Button>
          }
        />
      </SettingsCard>
    </SettingsSection>
  );
}
