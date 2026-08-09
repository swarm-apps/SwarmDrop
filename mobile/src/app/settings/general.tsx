import { Trans, useLingui } from "@lingui/react/macro";
import * as Device from "expo-device";
import { Bell, Folder } from "lucide-react-native";
import { useEffect, useState } from "react";
import { Pressable, View } from "react-native";
import { AppScreen } from "@/components/mobile/screen";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import {
  ensureNotificationPermission,
  openNotificationSettings,
} from "@/core/notifier";
import {
  pickReceiveLocation,
  requiresUserChoice,
  useReceiveLocation,
} from "@/core/receive-location";
import { useThemeColors } from "@/hooks/useThemeColors";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { lastPathSegment } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

export default function GeneralScreen() {
  const { t } = useLingui();

  const deviceName = Device.deviceName ?? Device.modelName ?? "—";

  return (
    <AppScreen
      scroll
      header={<SettingsHeader title={t`通用`} />}
      contentClassName="gap-5 pt-2"
    >
      <SettingSection label={t`设备`}>
        <View className="flex-row items-center justify-between px-3.5 py-3">
          <Text className="text-[14px] text-foreground">
            <Trans>设备名</Trans>
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {deviceName}
          </Text>
        </View>
        <SettingDivider />
        <View className="flex-row items-center justify-between px-3.5 py-3">
          <Text className="text-[14px] text-foreground">
            <Trans>型号</Trans>
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {Device.modelName ?? "—"}
          </Text>
        </View>
        <SettingDivider />
        <View className="flex-row items-center justify-between px-3.5 py-3">
          <Text className="text-[14px] text-foreground">
            <Trans>系统</Trans>
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {Device.osName} {Device.osVersion ?? ""}
          </Text>
        </View>
      </SettingSection>

      <SettingSection label={t`传输`}>
        <PauseReceivingRow />
        <SettingDivider />
        <ReceivePathRow />
      </SettingSection>

      <SettingSection label={t`通知`}>
        <NotificationRow />
      </SettingSection>
    </AppScreen>
  );
}

/**
 * 全局「暂停接收」开关。暂停期间节点保持在线可发现、配对不受影响,但对新 offer 自动婉拒。
 * 依赖 mobile-core 的 set/isReceivingPaused 绑定;旧原生(无该绑定)时整行隐藏,避免误导。
 */
function PauseReceivingRow() {
  const { t } = useLingui();
  const runtimeState = useMobileCoreStore((s) => s.runtimeState);
  const nodeRunning = runtimeState === "running";
  const [paused, setPaused] = useState(false);
  const [available, setAvailable] = useState(false);
  const [busy, setBusy] = useState(false);

  // 节点状态变化(如刚启动/关闭)时重读真实暂停态。
  // biome-ignore lint/correctness/useExhaustiveDependencies: runtimeState 仅作为重读触发器。
  useEffect(() => {
    const core = getMobileCore();
    if (typeof core.isReceivingPaused !== "function") {
      setAvailable(false);
      return;
    }
    setAvailable(true);
    let cancelled = false;
    core
      .isReceivingPaused()
      .then((value) => {
        if (!cancelled) setPaused(value);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [runtimeState]);

  const onToggle = async (next: boolean) => {
    const core = getMobileCore();
    if (typeof core.setReceivingPaused !== "function" || busy || !nodeRunning) {
      return;
    }
    setBusy(true);
    setPaused(next); // 乐观更新
    try {
      await core.setReceivingPaused(next);
    } catch (err) {
      setPaused(!next); // 失败回滚
      toast.error(t`操作失败`, getErrorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  // 旧原生未导出该能力时不渲染(graceful degradation)。
  if (!available) return null;

  return (
    <View className="flex-row items-center justify-between gap-3 px-3.5 py-3">
      <View className="flex-1 gap-0.5">
        <Text className="text-[14px] text-foreground">
          <Trans>暂停接收</Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {nodeRunning ? (
            <Trans>
              暂停期间节点仍在线可发现、可配对，但会自动婉拒新的接收请求。
            </Trans>
          ) : (
            <Trans>节点未启动，启动节点后即可暂停接收。</Trans>
          )}
        </Text>
      </View>
      <Switch
        checked={paused}
        disabled={busy}
        onCheckedChange={onToggle}
        accessibilityLabel={t`暂停接收`}
        testID="settings-pause-receiving-switch"
      />
    </View>
  );
}

/**
 * 通知提醒入口:先尝试请求权限,已被拒则深链到系统通知设置让用户手动开启
 *（权限被拒后 requestPermission 不再弹窗,故落到 openSettings 兜底)。
 */
function NotificationRow() {
  const { t } = useLingui();
  const colors = useThemeColors();

  const onPress = async () => {
    try {
      const granted = await ensureNotificationPermission();
      if (!granted) {
        await openNotificationSettings();
      } else {
        toast.success(t`通知已开启`);
      }
    } catch (err) {
      toast.error(t`操作失败`, getErrorMessage(err));
    }
  };

  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={t`通知提醒`}
      className="flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
      testID="settings-notification-row"
    >
      <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
        <Bell color={colors.mutedForeground} size={16} />
      </View>
      <View className="flex-1 gap-0.5">
        <Text className="text-[14px] text-foreground">
          <Trans>通知提醒</Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          <Trans>接收配对与文件传输请求的系统通知</Trans>
        </Text>
      </View>
    </Pressable>
  );
}

/**
 * 接收位置。两端形态不同，因为平台给的东西就不同：
 *
 * - **iOS** — 恒为 `Documents`，经文件共享出现在「文件」App 里。用户既无需也无从更改，
 *   所以这里是一行只读说明，不是一个入口。
 * - **Android** — 用户选定的 SAF 目录，可更换。**没有「恢复默认」**：应用私有目录自
 *   Android 11 起连系统文件管理器都进不去，把它当默认值等于把收到的文件锁进一座孤岛。
 */
function ReceivePathRow() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const location = useReceiveLocation();

  const onPickDirectory = async () => {
    const picked = await pickReceiveLocation();
    // 取消不作声：用户退出选择器就是不想换了。
    if (picked.outcome === "unusable") toast.error(t`此目录不可用`);
    else if (picked.outcome === "picked") toast.success(t`接收位置已更新`);
  };

  const body = (
    <>
      <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
        <Folder color={colors.mutedForeground} size={16} />
      </View>
      <View className="flex-1 gap-0.5">
        <Text className="text-[14px] text-foreground">
          <Trans>接收位置</Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
          {location.status === "ready" ? (
            lastPathSegment(location.uri)
          ) : (
            <Trans>尚未设置</Trans>
          )}
        </Text>
      </View>
    </>
  );

  if (!requiresUserChoice()) {
    return (
      <View className="flex-row items-center gap-3 px-3.5 py-3">{body}</View>
    );
  }

  return (
    <Pressable
      onPress={onPickDirectory}
      accessibilityRole="button"
      accessibilityLabel={t`接收位置`}
      className="flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
    >
      {body}
    </Pressable>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
