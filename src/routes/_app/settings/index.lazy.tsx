/**
 * Settings Page (Lazy)
 * 设置页面 - 懒加载组件
 *
 * 布局：bento 不规则卡片网格（lg 六列 / md 两列 / sm 单列）。按内容高度配对成行避免空洞：
 *   row1 满宽英雄（设备信息）· row2 关于产品介绍（满宽，置顶展示定位）
 *   row3 外观｜通用+传输竖叠｜网络（三者高度相近）· row4 引导节点｜MCP（两高卡对半）。
 *   标题由 AppTopBar 面包屑承担。
 */

import { createLazyFileRoute } from "@tanstack/react-router";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { useTheme } from "next-themes";
import { MonitorSmartphone, Palette } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useShallow } from "zustand/react/shallow";
import {
  usePreferencesStore,
  type CloseBehavior,
} from "@/stores/preferences-store";
import { locales, type LocaleKey } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { AboutSection } from "./-about-section";
import { DeviceInfoSection } from "./-device-info-section";
import { NetworkSettingsSection } from "./-network-settings-section";
import { BootstrapNodesSection } from "./-bootstrap-nodes-section";
import { TransferSettingsSection } from "./-transfer-settings-section";
import { McpSection } from "./-mcp-section";
import {
  SettingsCard,
  SettingsRow,
  SettingsSection,
} from "./-settings-primitives";

export const Route = createLazyFileRoute("/_app/settings/")({
  component: SettingsPage,
});

const themeOptions = [
  { value: "system", label: msg`跟随系统` },
  { value: "light", label: msg`浅色` },
  { value: "dark", label: msg`深色` },
];

const closeBehaviorOptions = [
  { value: "ask", label: msg`每次询问` },
  { value: "tray", label: msg`最小化到托盘` },
  { value: "quit", label: msg`退出应用` },
];

function SettingsPage() {
  return (
    <main className="settings-workbench flex h-full min-h-0 flex-1 flex-col bg-transparent">
      <div className="flex-1 overflow-auto p-4 md:p-5 lg:p-6">
        <div className="mx-auto max-w-[1040px]">
          <div className="grid grid-cols-1 items-stretch gap-4 md:grid-cols-2 md:gap-5 lg:grid-cols-6">
            {/* row1 英雄：设备信息（满宽横卡） */}
            <div id="device" className="scroll-mt-6 md:col-span-2 lg:col-span-6">
              <DeviceInfoSection />
            </div>

            {/* row2 关于：产品介绍（满宽，置顶展示产品定位） */}
            <div id="about" className="scroll-mt-6 md:col-span-2 lg:col-span-6">
              <AboutSection />
            </div>

            {/* row3 偏好：外观｜通用+文件传输竖叠｜网络（三者高度相近，齐平） */}
            <div id="appearance" className="scroll-mt-6 lg:col-span-2">
              <AppearanceSection />
            </div>
            <div className="flex flex-col gap-4 md:gap-5 lg:col-span-2">
              <div id="general" className="scroll-mt-6">
                <GeneralSection />
              </div>
              <div id="transfer" className="flex flex-1 flex-col scroll-mt-6">
                <TransferSettingsSection />
              </div>
            </div>
            <div
              id="network"
              className="scroll-mt-6 md:col-span-2 lg:col-span-2"
            >
              <NetworkSettingsSection />
            </div>

            {/* row4 网络进阶：引导节点｜MCP（两高卡对半，齐平） */}
            <div
              id="bootstrap"
              className="scroll-mt-6 md:col-span-2 lg:col-span-3"
            >
              <BootstrapNodesSection />
            </div>
            <div id="mcp" className="scroll-mt-6 md:col-span-2 lg:col-span-3">
              <McpSection />
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}

/** 通用设置：窗口关闭行为等桌面端偏好 */
function GeneralSection() {
  const { t } = useLingui();
  const { closeBehavior, setCloseBehavior } = usePreferencesStore(
    useShallow((state) => ({
      closeBehavior: state.closeBehavior,
      setCloseBehavior: state.setCloseBehavior,
    })),
  );

  return (
    <SettingsSection title={<Trans>通用</Trans>} icon={MonitorSmartphone}>
      <SettingsCard>
        <SettingsRow
          title={<Trans>关闭主窗口时</Trans>}
          description={<Trans>点窗口关闭按钮（✕）后的行为</Trans>}
          action={
            <Select
              value={closeBehavior}
              onValueChange={(value) =>
                setCloseBehavior(value as CloseBehavior)
              }
            >
              <SelectTrigger className="w-30 shrink-0 sm:w-35">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {closeBehaviorOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {t(option.label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
      </SettingsCard>
    </SettingsSection>
  );
}

/** 外观设置：可视化主题选择 + 语言 */
function AppearanceSection() {
  const { t } = useLingui();
  const { theme, setTheme } = useTheme();
  const { locale, setLocale } = usePreferencesStore(
    useShallow((state) => ({
      locale: state.locale,
      setLocale: state.setLocale,
    })),
  );

  return (
    <SettingsSection title={<Trans>外观</Trans>} icon={Palette} fill>
      <SettingsCard fill>
        <div className="flex flex-col gap-3 p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>主题</Trans>
            </span>
            <span className="text-xs leading-5 text-muted-foreground">
              <Trans>选择应用的外观主题</Trans>
            </span>
          </div>
          <div className="grid grid-cols-3 gap-2">
            {themeOptions.map((option) => (
              <ThemeOption
                key={option.value}
                variant={option.value as ThemeVariant}
                label={t(option.label)}
                active={theme === option.value}
                onSelect={() => setTheme(option.value)}
              />
            ))}
          </div>
        </div>

        <SettingsRow
          title={<Trans>语言</Trans>}
          description={<Trans>选择应用显示语言</Trans>}
          action={
            <Select
              value={locale}
              onValueChange={(value) => setLocale(value as LocaleKey)}
            >
              <SelectTrigger className="w-30 shrink-0 sm:w-35">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(locales).map(([key, label]) => (
                  <SelectItem key={key} value={key}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          }
        />
      </SettingsCard>
    </SettingsSection>
  );
}

type ThemeVariant = "system" | "light" | "dark";

/** 主题选项：迷你预览缩略图 + 标签 + 选中态 */
function ThemeOption({
  variant,
  label,
  active,
  onSelect,
}: {
  variant: ThemeVariant;
  label: string;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      className={cn(
        "flex flex-col items-center gap-2 rounded-xl border p-2 transition-colors",
        active
          ? "border-blue-500/60 bg-blue-500/5"
          : "border-border/70 bg-background/40 hover:border-border hover:bg-accent/40",
      )}
    >
      <ThemePreview variant={variant} />
      <span
        className={cn(
          "text-xs font-medium",
          active ? "text-blue-600 dark:text-blue-400" : "text-muted-foreground",
        )}
      >
        {label}
      </span>
    </button>
  );
}

/** 主题迷你预览：固定展示该主题的样子，不随当前主题切换 */
function ThemePreview({ variant }: { variant: ThemeVariant }) {
  if (variant === "system") {
    return (
      <div className="flex h-10 w-full overflow-hidden rounded-lg border border-border/70">
        <div className="flex-1 space-y-1 bg-white p-1.5">
          <div className="h-1 w-3/4 rounded-full bg-black/20" />
          <div className="h-1 w-1/2 rounded-full bg-black/10" />
        </div>
        <div className="flex-1 space-y-1 bg-zinc-900 p-1.5">
          <div className="h-1 w-3/4 rounded-full bg-white/30" />
          <div className="h-1 w-1/2 rounded-full bg-white/20" />
        </div>
      </div>
    );
  }

  const isDark = variant === "dark";
  return (
    <div
      className={cn(
        "h-10 w-full overflow-hidden rounded-lg border",
        isDark ? "border-white/10 bg-zinc-900" : "border-black/10 bg-white",
      )}
    >
      <div
        className={cn(
          "flex h-2.5 items-center gap-0.5 border-b px-1",
          isDark ? "border-white/10" : "border-black/5",
        )}
      >
        <span
          className={cn(
            "size-1 rounded-full",
            isDark ? "bg-white/25" : "bg-black/15",
          )}
        />
        <span
          className={cn(
            "size-1 rounded-full",
            isDark ? "bg-white/15" : "bg-black/10",
          )}
        />
      </div>
      <div className="space-y-1 p-1.5">
        <div
          className={cn(
            "h-1 w-2/3 rounded-full",
            isDark ? "bg-white/25" : "bg-black/15",
          )}
        />
        <div
          className={cn(
            "h-1 w-1/2 rounded-full",
            isDark ? "bg-white/15" : "bg-black/10",
          )}
        />
      </div>
    </div>
  );
}
