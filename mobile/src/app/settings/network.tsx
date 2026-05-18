import { Trans, useLingui } from "@lingui/react/macro";
import { Fragment } from "react";
import { ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useShallow } from "zustand/react/shallow";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { StatusPill } from "@/components/status-pill";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function NetworkScreen() {
  const { t } = useLingui();
  const { networkStatus, runtimeState } = useMobileCoreStore(
    useShallow((s) => ({
      networkStatus: s.networkStatus,
      runtimeState: s.runtimeState,
    })),
  );
  const { autoStart, setAutoStart } = usePreferencesStore(
    useShallow((s) => ({
      autoStart: s.autoStart,
      setAutoStart: s.setAutoStart,
    })),
  );

  const rows: Array<{ key: string; label: React.ReactNode; value: string }> = [
    {
      key: "connected",
      label: <Trans>已连接节点</Trans>,
      value: String(networkStatus?.connectedPeers ?? 0),
    },
    {
      key: "discovered",
      label: <Trans>已发现节点</Trans>,
      value: String(networkStatus?.discoveredPeers ?? 0),
    },
    {
      key: "nat",
      label: <Trans>NAT 状态</Trans>,
      value: networkStatus?.natStatus ?? "—",
    },
    {
      key: "relay",
      label: <Trans>中继</Trans>,
      value: networkStatus?.relayReady ? t`就绪` : t`未启用`,
    },
  ];

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`网络`} />
      <ScrollView
        contentContainerClassName="gap-5 px-5 pt-2 pb-8"
        showsVerticalScrollIndicator={false}
      >
        <SettingSection label={t`通用`}>
          <View className="flex-row items-center justify-between px-3.5 py-3 gap-3">
            <View className="flex-1">
              <Text className="text-[14px] text-foreground">
                <Trans>自动启动节点</Trans>
              </Text>
              <Text className="mt-0.5 text-[11px] text-muted-foreground">
                <Trans>App 启动后自动启动 P2P 节点</Trans>
              </Text>
            </View>
            <Switch checked={autoStart} onCheckedChange={setAutoStart} />
          </View>
        </SettingSection>

        <SettingSection label={t`节点状态`}>
          <View className="flex-row items-center justify-between px-3.5 py-3">
            <Text className="text-[14px] text-foreground">
              <Trans>当前状态</Trans>
            </Text>
            <StatusPill state={runtimeState} />
          </View>
        </SettingSection>

        <SettingSection label={t`详细信息`}>
          {rows.map((row, idx) => (
            <Fragment key={row.key}>
              <View className="flex-row items-center justify-between px-3.5 py-3">
                <Text className="text-[14px] text-foreground">{row.label}</Text>
                <Text
                  className="text-[13px] text-muted-foreground"
                  numberOfLines={1}
                >
                  {row.value}
                </Text>
              </View>
              {idx < rows.length - 1 ? <SettingDivider /> : null}
            </Fragment>
          ))}
        </SettingSection>
      </ScrollView>
    </SafeAreaView>
  );
}
