import { useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import {
  ChevronRight,
  Globe,
  Info,
  Languages,
  type LucideIcon,
  Network,
  Palette,
  Settings as SettingsIcon,
} from "lucide-react-native";
import { Pressable, View } from "react-native";
import { DeviceInfoCard } from "@/components/device-info-card";
import { AppHeader, AppScreen } from "@/components/mobile/screen";
import { SettingDivider } from "@/components/setting-row";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function MainSettingsScreen() {
  const router = useRouter();
  const { t } = useLingui();

  return (
    <AppScreen scroll testID="settings-screen" contentClassName="gap-4 pt-1">
      <AppHeader
        title={t`设置`}
        subtitle={t`应用、网络、外观和设备信息`}
        testID="settings-header"
      />

      <DeviceInfoCard />

      <View className="overflow-hidden rounded-lg border border-border bg-card">
        <NavRow
          icon={SettingsIcon}
          label={t`通用`}
          onPress={() => router.push("/settings/general" as never)}
        />
        <SettingDivider />
        <NavRow
          icon={Languages}
          label={t`语言`}
          onPress={() => router.push("/settings/language" as never)}
        />
        <SettingDivider />
        <NavRow
          icon={Palette}
          label={t`外观`}
          onPress={() => router.push("/settings/theme" as never)}
        />
        <SettingDivider />
        <NavRow
          icon={Globe}
          label={t`网络`}
          onPress={() => router.push("/settings/network" as never)}
        />
        <SettingDivider />
        <NavRow
          icon={Network}
          label={t`引导节点`}
          onPress={() => router.push("/settings/bootstrap-nodes" as never)}
        />
        <SettingDivider />
        <NavRow
          icon={Info}
          label={t`关于`}
          onPress={() => router.push("/settings/about" as never)}
        />
      </View>
    </AppScreen>
  );
}

function NavRow({
  icon: Icon,
  label,
  onPress,
}: {
  icon: LucideIcon;
  label: string;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      className="min-h-12 flex-row items-center gap-3 px-3.5 active:bg-muted"
    >
      <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
        <Icon color={colors.mutedForeground} size={16} />
      </View>
      <Text className="flex-1 text-[14px] text-foreground">{label}</Text>
      <ChevronRight color={colors.mutedForeground} size={16} />
    </Pressable>
  );
}
