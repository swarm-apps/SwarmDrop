/**
 * Settings Page (Lazy)
 * 设置页面 - 懒加载组件
 */

import { createLazyFileRoute } from "@tanstack/react-router";
import type { ComponentType, ReactNode } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { useTheme } from "next-themes";
import {
  MonitorSmartphone,
  Palette,
  ShieldCheck,
} from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useShallow } from "zustand/react/shallow";
import { usePreferencesStore } from "@/stores/preferences-store";
import { locales, type LocaleKey } from "@/lib/i18n";
import { AboutPanel } from "./-about-section";
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

const showHeroSummaryPills = false;

function SettingsPage() {
  const { t } = useLingui();
  const { theme, setTheme } = useTheme();
  const { locale, setLocale } = usePreferencesStore(
    useShallow((state) => ({
      locale: state.locale,
      setLocale: state.setLocale,
    }))
  );

  return (
    <main className="settings-workbench flex h-full min-h-0 flex-1 flex-col bg-transparent">
      {/* Page Content —— 标题由 AppTopBar 面包屑承担 */}
      <div className="flex-1 overflow-auto p-4 md:p-5 lg:p-6">
        <div className="mx-auto max-w-[1240px]">
          <div className="settings-mosaic min-w-0">
            <SettingsHero theme={theme} locale={locale} />

            <div className="settings-board">
              {/* 设备信息 */}
              <div id="device" className="scroll-mt-6">
                <DeviceInfoSection />
              </div>

              {/* 外观 */}
              <div id="appearance" className="scroll-mt-6">
                <SettingsSection title={<Trans>外观</Trans>} icon={Palette}>
                  <SettingsCard>
                    <SettingsRow
                      title={<Trans>主题</Trans>}
                      description={<Trans>选择应用的外观主题</Trans>}
                      action={
                        <Select value={theme} onValueChange={setTheme}>
                          <SelectTrigger className="w-30 shrink-0 sm:w-35">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            {themeOptions.map((option) => (
                              <SelectItem key={option.value} value={option.value}>
                                {t(option.label)}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      }
                    />

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
              </div>

              {/* 文件传输设置 */}
              <div id="transfer" className="scroll-mt-6">
                <TransferSettingsSection />
              </div>

              {/* 网络 */}
              <div id="network" className="scroll-mt-6">
                <NetworkSettingsSection />
              </div>

              {/* 引导节点 */}
              <div id="bootstrap" className="scroll-mt-6">
                <BootstrapNodesSection />
              </div>

              {/* MCP Server */}
              <div id="mcp" className="scroll-mt-6">
                <McpSection />
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}

function SettingsHero({
  theme,
  locale,
}: {
  theme: string | undefined;
  locale: LocaleKey;
}) {
  const themeLabel =
    theme === "dark" ? (
      <Trans>深色</Trans>
    ) : theme === "light" ? (
      <Trans>浅色</Trans>
    ) : (
      <Trans>跟随系统</Trans>
  );

  return (
    <section id="about" className="settings-hero glass-panel mx-auto w-full max-w-[1240px] scroll-mt-6 overflow-hidden rounded-[26px] p-1.5">
      <div className="flex flex-col gap-4 rounded-[21px] bg-white/30 p-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.48)] dark:bg-white/[0.035] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.06)] lg:p-5">
        <div className="settings-hero-content-rail">
          <div className="flex min-w-0 items-start gap-3.5">
            <span className="glass-control flex size-12 shrink-0 items-center justify-center rounded-[18px]">
              <img src="/app-icon.svg" alt="SwarmDrop" className="size-8 rounded-[10px]" />
            </span>
            <div className="min-w-0">
              <h1 className="text-[24px] font-semibold leading-none tracking-tight text-foreground sm:text-[28px]">
                <Trans>关于</Trans>
              </h1>
              <p className="mt-1.5 max-w-[54ch] text-sm leading-6 text-muted-foreground">
                <Trans>查看版本、更新状态与运行环境，下方可继续调整设备、网络和接收设置。</Trans>
              </p>
            </div>
          </div>

          <div className="settings-hero-about-stack">
            <AboutPanel
              variant="hero"
              className="settings-hero-about-card"
            />
            {/* 暂时隐藏底部主题 / 平台 / 语言概览。恢复时将 showHeroSummaryPills 改为 true。 */}
            {showHeroSummaryPills ? (
              <div className="settings-hero-summary-grid w-full">
                <OverviewPill
                  icon={Palette}
                  label={<Trans>主题</Trans>}
                  value={themeLabel}
                />
                <OverviewPill
                  icon={MonitorSmartphone}
                  label={<Trans>平台</Trans>}
                  value={<Trans>桌面端</Trans>}
                />
                <OverviewPill
                  icon={ShieldCheck}
                  label={<Trans>语言</Trans>}
                  value={locales[locale]}
                />
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );
}

function OverviewPill({
  icon: Icon,
  label,
  value,
}: {
  icon: ComponentType<{ className?: string }>;
  label: ReactNode;
  value: ReactNode;
}) {
  return (
    <div className="glass-control flex items-center justify-between gap-3 rounded-[16px] px-3 py-2.5">
      <div className="flex min-w-0 items-center gap-2">
        <Icon className="size-4 shrink-0 text-muted-foreground" />
        <span className="text-xs text-muted-foreground">{label}</span>
      </div>
      <span className="truncate text-sm font-semibold text-foreground">{value}</span>
    </div>
  );
}
