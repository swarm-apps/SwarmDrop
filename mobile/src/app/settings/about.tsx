import { Trans, useLingui } from "@lingui/react/macro";
import type { UpdateStatus } from "@swarm-hive/sdk";
import Constants from "expo-constants";
import type { LucideIcon } from "lucide-react-native";
import {
  ArrowUpRight,
  BadgeCheck,
  BookText,
  Code,
  Download,
  FileText,
  KeyRound,
  MessageSquare,
  RefreshCw,
  ScrollText,
  ShieldCheck,
  Waypoints,
} from "lucide-react-native";
import type { ReactNode } from "react";
import {
  ActivityIndicator,
  Image,
  Linking,
  Platform,
  View,
} from "react-native";
import { logFilePath } from "react-native-swarmdrop-core";
import { AppScreen } from "@/components/mobile/screen";
import {
  SettingDivider,
  SettingRow,
  SettingSection,
} from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import { useThemeColors } from "@/hooks/useThemeColors";
import { shareFileWithSystem } from "@/lib/open-file";
import { toast } from "@/lib/toast";
import { updateActionKind } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts } from "@/lib/update-texts";

const APP_VERSION = Constants.expoConfig?.version ?? "0.0.0";

export default function AboutScreen() {
  const colors = useThemeColors();
  const { t } = useLingui();
  const { status, check } = useUpdate();
  const { install } = useAutoInstall();

  const isAndroid = Platform.OS === "android";

  const openUrl = (url: string) => {
    Linking.openURL(url).catch((err) => {
      console.warn("[about] openURL failed:", err);
      toast.error(t`无法打开链接`, err);
    });
  };

  /**
   * 「软件更新」行的标签 / 动作 / 右侧状态，由 status **穷尽**推出（见 updateActionKind）。
   *
   * 从前这里是 `hasUpdate ? … : isChecking ? … : isError ? … : 已是最新` 的三元链：
   * `downloading` 与 `ready` 双双掉进最后那个兜底分支，于是产物已下好等着装的时候，
   * 这一行却显示「✅ 已是最新」，而前面正压着一个说有新版本要装的弹窗（v0.12.3 现场）。
   *
   * `onPress` 在不可操作的状态下必须是 `undefined` 而非空函数 —— SettingRow 据它切换
   * Pressable/View 与 accessibilityRole，给个空函数会让这一行对读屏用户自称按钮。
   */
  const updateAction = updateActionKind(status);
  const isUpdateBusy =
    updateAction === "checking" || updateAction === "downloading";
  const updateRowLabel =
    updateAction === "install" || updateAction === "downloading"
      ? t`软件更新`
      : t`检查更新`;
  const onUpdateRowPress = isUpdateBusy
    ? undefined
    : updateAction === "install"
      ? () => void install()
      : () => void check(true);

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
    <AppScreen
      scroll
      header={<SettingsHeader title={t`关于`} />}
      contentClassName="gap-6 pt-4 pb-10"
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
            icon={ShieldCheck}
            title={<Trans>端到端加密</Trans>}
            description={<Trans>Noise 或 TLS 1.3，中继全程只经手密文</Trans>}
          />
          <SecurityFeatureRow
            icon={KeyRound}
            title={<Trans>一次一密</Trans>}
            description={<Trans>每条连接独立握手，会话密钥仅存内存</Trans>}
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
            label={updateRowLabel}
            onPress={onUpdateRowPress}
          >
            <UpdateRowStatus status={status} colors={colors} />
          </SettingRow>
        </SettingSection>
      ) : null}

      <SettingSection label={t`资源`}>
        <LinkRow
          // 三端此处本该同为 `Github`，但 lucide-react-native 1.x 不带品牌图标
          // （lucide 已把它们移出核心集，桌面/Web 的 `Github` 是 lucide-react 里
          // 尚存的 deprecated 别名）。这是库能力差异，不是漏对齐。
          icon={Code}
          label="GitHub"
          onPress={() => openUrl("https://github.com/swarm-apps/SwarmDrop")}
        />
        <SettingDivider />
        <LinkRow
          icon={BookText}
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
          icon={ScrollText}
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
        <LinkRow icon={FileText} label={t`导出日志`} onPress={onExportLogs} />
      </SettingSection>
    </AppScreen>
  );
}

/**
 * 「软件更新」行右侧的状态徽标。三个分支共用同一个 badge 包装,所以它独立成组件 ——
 * 否则那行 `flex-row items-center gap-1` 要在每个分支里重写一遍。
 */
function UpdateRowStatus({
  status,
  colors,
}: {
  status: UpdateStatus;
  colors: ReturnType<typeof useThemeColors>;
}) {
  const updateTexts = resolveUpdateTexts();
  switch (updateActionKind(status)) {
    case "checking":
      return <ActivityIndicator color={colors.mutedForeground} size="small" />;
    case "download":
      return (
        <Badge tone="primary" colors={colors} icon={Download}>
          <Trans>有新版可用</Trans>
        </Badge>
      );
    case "downloading":
      return (
        <View className="flex-row items-center gap-1">
          <ActivityIndicator color={colors.mutedForeground} size="small" />
          <Text className="text-[13px] text-muted-foreground">
            <Trans>下载中</Trans>
          </Text>
        </View>
      );
    // 文案取自 update-texts,与弹窗、设置区说的是同一件事 —— 同屏时不能一个写「点击安装」
    // 一个写「立即安装」。其余状态是为设置行定制的短句,留在 Trans。
    case "install":
      return (
        <Badge tone="primary" colors={colors} icon={Download}>
          {updateTexts.installButton}
        </Badge>
      );
    // check 分支覆盖 idle / up-to-date / error 三态,只有 error 要换个说法。
    default:
      return status === "error" ? (
        <Text className="text-[13px] text-muted-foreground">
          <Trans>检查失败</Trans>
        </Text>
      ) : (
        <Badge tone="success" colors={colors} icon={BadgeCheck}>
          <Trans>已是最新</Trans>
        </Badge>
      );
  }
}

function Badge({
  tone,
  colors,
  icon: Icon,
  children,
}: {
  tone: "primary" | "success";
  colors: ReturnType<typeof useThemeColors>;
  icon: LucideIcon;
  children: ReactNode;
}) {
  return (
    <View className="flex-row items-center gap-1">
      <Icon
        color={tone === "primary" ? colors.primary : colors.success}
        size={12}
      />
      <Text
        className={
          tone === "primary"
            ? "text-[13px] font-medium text-primary-ink"
            : "text-[13px] font-medium text-success-ink"
        }
      >
        {children}
      </Text>
    </View>
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

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
