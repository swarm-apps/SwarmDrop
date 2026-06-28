import { useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import {
  ChevronRight,
  Globe,
  Info,
  Languages,
  type LucideIcon,
  Palette,
  Settings as SettingsIcon,
} from "lucide-react-native";
import { Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { DeviceInfoCard } from "@/components/device-info-card";
import { SettingDivider } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function SettingsIndex() {
  const router = useRouter();
  const { t } = useLingui();

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`设置`} />
      <ScrollView
        contentContainerClassName="gap-4 px-5 pt-2 pb-8"
        showsVerticalScrollIndicator={false}
      >
        <DeviceInfoCard />

        <View className="rounded-xl border border-border bg-card overflow-hidden">
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
            testID="settings-network-row"
            onPress={() => router.push("/settings/network" as never)}
          />
          <SettingDivider />
          <NavRow
            icon={Info}
            label={t`关于`}
            onPress={() => router.push("/settings/about" as never)}
          />
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function NavRow({
  icon: Icon,
  label,
  testID,
  onPress,
}: {
  icon: LucideIcon;
  label: string;
  testID?: string;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      testID={testID}
      className="h-12 flex-row items-center px-3.5 gap-3 active:bg-muted"
    >
      <View className="h-8 w-8 items-center justify-center rounded-lg bg-muted">
        <Icon color={colors.mutedForeground} size={16} />
      </View>
      <Text className="flex-1 text-[14px] text-foreground">{label}</Text>
      <ChevronRight color={colors.mutedForeground} size={16} />
    </Pressable>
  );
}
