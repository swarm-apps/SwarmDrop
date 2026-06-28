import { Trans, useLingui } from "@lingui/react/macro";
import * as Device from "expo-device";
import { Directory } from "expo-file-system";
import { Folder, RotateCcw } from "lucide-react-native";
import { useState } from "react";
import { Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { getMobilePaths } from "@/core/paths";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function GeneralScreen() {
  const { t } = useLingui();

  const deviceName = Device.deviceName ?? Device.modelName ?? "—";

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`通用`} />
      <ScrollView
        contentContainerClassName="gap-5 px-5 pt-2 pb-8"
        showsVerticalScrollIndicator={false}
      >
        <SettingSection label={t`设备`}>
          <View className="flex-row items-center justify-between px-3.5 py-3">
            <Text className="text-[14px] text-foreground">
              <Trans>设备名</Trans>
            </Text>
            <Text
              className="text-[13px] text-muted-foreground"
              numberOfLines={1}
            >
              {deviceName}
            </Text>
          </View>
          <SettingDivider />
          <View className="flex-row items-center justify-between px-3.5 py-3">
            <Text className="text-[14px] text-foreground">
              <Trans>型号</Trans>
            </Text>
            <Text
              className="text-[13px] text-muted-foreground"
              numberOfLines={1}
            >
              {Device.modelName ?? "—"}
            </Text>
          </View>
          <SettingDivider />
          <View className="flex-row items-center justify-between px-3.5 py-3">
            <Text className="text-[14px] text-foreground">
              <Trans>系统</Trans>
            </Text>
            <Text
              className="text-[13px] text-muted-foreground"
              numberOfLines={1}
            >
              {Device.osName} {Device.osVersion ?? ""}
            </Text>
          </View>
        </SettingSection>

        <SettingSection label={t`传输`}>
          <ReceivePathRow />
        </SettingSection>
      </ScrollView>
    </SafeAreaView>
  );
}

function ReceivePathRow() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const receivePath = usePreferencesStore((s) => s.receivePath);
  const setReceivePath = usePreferencesStore((s) => s.setReceivePath);
  const [resetOpen, setResetOpen] = useState(false);

  const onPickDirectory = async () => {
    try {
      const dir = await Directory.pickDirectoryAsync();
      // 只读探测：调一次 list() 验证 URI 持久化权限可用。
      // iOS file:// / Android SAF content:// 双平台都用同一套校验。
      // SAF 写入由 ForeignFileAccess 适配（expo-file-system 56 起支持 chunk write）。
      try {
        dir.list();
      } catch (probeErr) {
        toast.error(t`此目录不可读`, probeErr);
        return;
      }
      setReceivePath(dir.uri);
      toast.success(t`接收位置已更新`);
    } catch (err) {
      toast.error(t`选择失败`, err);
    }
  };

  const displayPath = receivePath ?? getMobilePaths().transfersInboxUri;
  const isCustom = receivePath !== null;

  return (
    <>
      <Pressable
        onPress={onPickDirectory}
        accessibilityRole="button"
        accessibilityLabel={t`接收位置`}
        className="flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
      >
        <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
          <Folder color={colors.mutedForeground} size={16} />
        </View>
        <View className="flex-1 gap-0.5">
          <Text className="text-[14px] text-foreground">
            <Trans>接收位置</Trans>
          </Text>
          <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
            {isCustom ? (
              prettyPath(displayPath)
            ) : (
              <Trans>应用私有目录（默认）</Trans>
            )}
          </Text>
        </View>
        {isCustom ? (
          <Pressable
            onPress={() => setResetOpen(true)}
            hitSlop={8}
            accessibilityRole="button"
            accessibilityLabel={t`恢复默认`}
            className="rounded-full p-1.5 active:bg-destructive/15"
          >
            <RotateCcw size={14} color={colors.mutedForeground} />
          </Pressable>
        ) : null}
      </Pressable>

      <ConfirmDialog
        open={resetOpen}
        onOpenChange={setResetOpen}
        title={<Trans>恢复默认接收位置</Trans>}
        description={<Trans>接收文件将保存到应用私有目录。</Trans>}
        actionLabel={<Trans>恢复默认</Trans>}
        destructive
        onAction={() => setReceivePath(null)}
      />
    </>
  );
}

/** 把 file:// 或 content:// URI 截成更短的显示串：取最后一段路径。 */
function prettyPath(uri: string): string {
  try {
    const decoded = decodeURIComponent(uri.replace(/\/$/, ""));
    const segments = decoded.split("/");
    const last = segments[segments.length - 1];
    return last && last.length > 0 ? last : decoded;
  } catch {
    return uri;
  }
}
