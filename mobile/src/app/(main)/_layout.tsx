import { useLingui } from "@lingui/react/macro";
import { NativeTabs } from "expo-router/unstable-native-tabs";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function MainLayout() {
  const { t } = useLingui();
  const colors = useThemeColors();

  return (
    <NativeTabs
      backgroundColor={colors.card}
      disableIndicator
      disableTransparentOnScrollEdge
      iconColor={{
        default: colors.mutedForeground,
        selected: colors.primary,
      }}
      indicatorColor={colors.primary}
      labelStyle={{
        default: {
          color: colors.mutedForeground,
          fontSize: 11,
          fontWeight: "600",
        },
        selected: {
          color: colors.primary,
          fontSize: 11,
          fontWeight: "600",
        },
      }}
      labelVisibilityMode="labeled"
      rippleColor="transparent"
      shadowColor={colors.border}
    >
      <NativeTabs.Trigger name="index">
        <NativeTabs.Trigger.Label>{t`设备`}</NativeTabs.Trigger.Label>
        {/* 互联节点(P2P 设备互连)—— 方正均衡,替换又高又窄的单台竖手机。
            选中态靠 iconColor 变色区分,故 default/selected 同一字形(与旧手机 tab 一致)。 */}
        <NativeTabs.Trigger.Icon
          md={{ default: "device_hub", selected: "device_hub" }}
          sf={{
            default: "point.3.connected.trianglepath.dotted",
            selected: "point.3.connected.trianglepath.dotted",
          }}
        />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="inbox">
        <NativeTabs.Trigger.Label>{t`收件箱`}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon
          md={{ default: "inbox", selected: "inbox" }}
          sf={{ default: "tray", selected: "tray.fill" }}
        />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Label>{t`设置`}</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon
          md={{ default: "settings", selected: "settings" }}
          sf={{ default: "gearshape", selected: "gearshape.fill" }}
        />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
