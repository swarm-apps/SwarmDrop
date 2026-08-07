import { Trans, useLingui } from "@lingui/react/macro";
import Constants from "expo-constants";
import type { LucideIcon } from "lucide-react-native";
import {
  ArrowUpRight,
  BadgeCheck,
  BookOpen,
  Code,
  Download,
  FileText,
  KeyRound,
  Lock,
  MessageSquare,
  RefreshCw,
  ScrollText,
  Waypoints,
} from "lucide-react-native";
import {
  ActivityIndicator,
  Image,
  Linking,
  Platform,
  ScrollView,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { logFilePath } from "react-native-swarmdrop-core";
import {
  SettingDivider,
  SettingRow,
  SettingSection,
} from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useUpdate } from "@/hooks/use-update";
import { useThemeColors } from "@/hooks/useThemeColors";
import { shareFileWithSystem } from "@/lib/open-file";
import { toast } from "@/lib/toast";

const APP_VERSION = Constants.expoConfig?.version ?? "0.0.0";

export default function AboutScreen() {
  const colors = useThemeColors();
  const { t } = useLingui();
  const { status, check } = useUpdate();

  const isAndroid = Platform.OS === "android";
  const isChecking = status === "checking";
  const isError = status === "error";
  const hasUpdate = status === "available" || status === "force-required";

  const openUrl = (url: string) => {
    Linking.openURL(url).catch((err) => {
      console.warn("[about] openURL failed:", err);
      toast.error(t`无法打开链接`, err);
    });
  };

  const onCheckUpdate = () => {
    if (isChecking) return;
    void check(true);
  };

  /**
   * 导出日志到系统分享面板。
   *
   * `logFilePath()` 返回 `file://` URI；未初始化或尚未写出文件时返回 undefined，
   * 此时给明确空态而不是拉起一个分享不了任何东西的面板。
   */
  const onExportLogs = () => {
    const uri = logFilePath();
    if (uri === undefined) {
      toast.error(t`暂无日志`, t`日志尚未生成，请先使用一段时间再试`);
      return;
    }
    const fileName = uri.split("/").pop() ?? "swarmdrop.log";
    void shareFileWithSystem(uri, fileName, t`导出日志`).catch((err) => {
      console.warn("[about] share log failed:", err);
      toast.error(t`导出日志失败`, err);
    });
  };

  return (
    <SafeAreaView style={{ flex: 1 }} className="bg-background" edges={["top"]}>
      <SettingsHeader title={t`关于`} />

      <ScrollView
        showsVerticalScrollIndicator={false}
        contentContainerClassName="gap-6 px-5 pt-4 pb-10"
      >
        {/* hero:左对齐 lockup,延续设置栈的编辑式排版,不做整页居中名片 */}
        <View className="gap-3">
          <View className="flex-row items-center gap-4">
            <Image
              source={require("../../../assets/images/icon.png")}
              className="h-16 w-16 rounded-2xl border border-border"
              accessibilityIgnoresInvertColors
            />
            <View className="gap-1.5">
              <Text className="text-[18px] font-semibold tracking-tight text-foreground">
                SwarmDrop
              </Text>
              <View className="self-start rounded-full bg-muted px-2.5 py-1">
                <Text className="font-mono text-[12px] text-muted-foreground">
                  v{APP_VERSION}
                </Text>
              </View>
            </View>
          </View>
          <Text className="text-[13px] leading-5 text-muted-foreground">
            <Trans>去中心化、跨网络、端到端加密文件传输</Trans>
          </Text>
        </View>

        {/* 安全与加密:加密是常量不是状态,协议名只在这里出现一次 */}
        <SettingSection label={t`安全与加密`}>
          <View className="gap-3.5 p-3.5">
            <SecurityFeatureRow
              icon={Lock}
              title={<Trans>端到端加密</Trans>}
              description={
                <Trans>Noise 或 TLS 1.3，中继全程只经手密文</Trans>
              }
            />
            <SecurityFeatureRow
              icon={KeyRound}
              title={<Trans>一次一密</Trans>}
              description={
                <Trans>每条连接独立握手，会话密钥仅存内存</Trans>
              }
            />
            <SecurityFeatureRow
              icon={Waypoints}
              title={<Trans>点对点直连</Trans>}
              description={<Trans>明文不经过任何服务器</Trans>}
            />
          </View>
        </SettingSection>

        {/* 应用内更新通道只存在于 Android,iOS 不渲染整个分组("检查失败"是噪音) */}
        {isAndroid ? (
          <SettingSection label={t`软件更新`}>
            <SettingRow
              icon={RefreshCw}
              label={t`检查更新`}
              onPress={onCheckUpdate}
            >
              {hasUpdate ? (
                <View className="flex-row items-center gap-1">
                  <Download color={colors.primary} size={12} />
                  <Text className="text-[13px] font-medium text-primary-ink">
                    <Trans>有新版可用</Trans>
                  </Text>
                </View>
              ) : isChecking ? (
                <ActivityIndicator
                  color={colors.mutedForeground}
                  size="small"
                />
              ) : isError ? (
                <Text className="text-[13px] text-muted-foreground">
                  <Trans>检查失败</Trans>
                </Text>
              ) : (
                <View className="flex-row items-center gap-1">
                  <BadgeCheck color={colors.success} size={12} />
                  <Text className="text-[13px] font-medium text-success-ink">
                    <Trans>已是最新</Trans>
                  </Text>
                </View>
              )}
            </SettingRow>
          </SettingSection>
        ) : null}

        <SettingSection label={t`资源`}>
          <LinkRow
            icon={Code}
            label="GitHub"
            onPress={() => openUrl("https://github.com/swarm-apps/SwarmDrop")}
          />
          <SettingDivider />
          <LinkRow
            icon={BookOpen}
            label={t`文档`}
            onPress={() => openUrl("https://swarm-apps.github.io/SwarmDrop/")}
          />
          <SettingDivider />
          <LinkRow
            icon={MessageSquare}
            label={t`反馈`}
            onPress={() =>
              openUrl("https://github.com/swarm-apps/SwarmDrop/issues")
            }
          />
          <SettingDivider />
          <LinkRow
            icon={FileText}
            label={t`更新日志`}
            onPress={() =>
              openUrl("https://github.com/swarm-apps/SwarmDrop/releases")
            }
          />
        </SettingSection>

        {/* 诊断日志。说明常驻在入口上方而非点击后弹窗——用户在按下去之前就该
            知道要分享出去的是什么。 */}
        <SettingSection label={t`诊断`}>
          <View className="px-3.5 pt-3 pb-1">
            <Text className="text-[12px] leading-4 text-muted-foreground">
              <Trans>
                遇到问题时，日志能帮我们定位。它包含设备 ID
                与网络地址，发送前请自行过目。
              </Trans>
            </Text>
          </View>
          <LinkRow
            icon={ScrollText}
            label={t`导出日志`}
            onPress={onExportLogs}
          />
        </SettingSection>
      </ScrollView>
    </SafeAreaView>
  );
}

function SecurityFeatureRow({
  icon: Icon,
  title,
  description,
}: {
  icon: LucideIcon;
  title: React.ReactNode;
  description: React.ReactNode;
}) {
  const colors = useThemeColors();
  return (
    <View className="flex-row items-center gap-3">
      <View className="size-9 items-center justify-center rounded-full bg-primary/10">
        <Icon color={colors.primary} size={16} />
      </View>
      <View className="min-w-0 flex-1 gap-0.5">
        <Text className="text-[13px] font-medium text-foreground">{title}</Text>
        <Text className="text-[12px] leading-4 text-muted-foreground">
          {description}
        </Text>
      </View>
    </View>
  );
}

function LinkRow({
  icon,
  label,
  onPress,
}: {
  icon: LucideIcon;
  label: string;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  return (
    <SettingRow icon={icon} label={label} onPress={onPress}>
      <ArrowUpRight color={colors.mutedForeground} size={14} />
    </SettingRow>
  );
}
