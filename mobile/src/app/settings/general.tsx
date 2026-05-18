import { Trans, useLingui } from "@lingui/react/macro";
import * as Device from "expo-device";
import { ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";

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
      </ScrollView>
    </SafeAreaView>
  );
}
