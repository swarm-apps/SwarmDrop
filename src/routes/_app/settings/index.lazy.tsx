/**
 * Settings Page (Lazy)
 * 设置页面 - 懒加载组件
 */

import { createLazyFileRoute } from "@tanstack/react-router";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { useTheme } from "next-themes";
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
import { AboutSection } from "./-about-section";
import { DeviceInfoSection } from "./-device-info-section";
import { NetworkSettingsSection } from "./-network-settings-section";
import { BootstrapNodesSection } from "./-bootstrap-nodes-section";
import { TransferSettingsSection } from "./-transfer-settings-section";
import { McpSection } from "./-mcp-section";

export const Route = createLazyFileRoute("/_app/settings/")({
  component: SettingsPage,
});

const themeOptions = [
  { value: "system", label: msg`跟随系统` },
  { value: "light", label: msg`浅色` },
  { value: "dark", label: msg`深色` },
];

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
    <main className="flex h-full min-h-0 flex-1 flex-col bg-transparent">
      {/* Page Content —— 标题由 AppTopBar 面包屑承担 */}
      <div className="flex-1 overflow-auto p-4 md:p-5 lg:p-6">
        <div className="mx-auto flex max-w-2xl flex-col gap-6">
          {/* 关于 & 更新 */}
          <AboutSection />

          {/* 设备信息 */}
          <DeviceInfoSection />

          {/* 网络 */}
          <NetworkSettingsSection />

          {/* 引导节点 */}
          <BootstrapNodesSection />

          {/* 外观 */}
          <section className="flex flex-col gap-3">
            <h2 className="text-sm font-semibold text-foreground">
              <Trans>外观</Trans>
            </h2>
            <div className="glass-card rounded-lg">
              {/* 主题 */}
              <div className="flex items-center justify-between border-b border-border p-4">
                <div className="flex flex-col gap-0.5">
                  <span className="text-sm font-medium text-foreground">
                    <Trans>主题</Trans>
                  </span>
                  <span className="text-xs text-muted-foreground">
                    <Trans>选择应用的外观主题</Trans>
                  </span>
                </div>
                <Select value={theme} onValueChange={setTheme}>
                  <SelectTrigger className="w-30 sm:w-35">
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
              </div>

              {/* 语言 */}
              <div className="flex items-center justify-between p-4">
                <div className="flex flex-col gap-0.5">
                  <span className="text-sm font-medium text-foreground">
                    <Trans>语言</Trans>
                  </span>
                  <span className="text-xs text-muted-foreground">
                    <Trans>选择应用显示语言</Trans>
                  </span>
                </div>
                <Select
                  value={locale}
                  onValueChange={(value) => setLocale(value as LocaleKey)}
                >
                  <SelectTrigger className="w-30 sm:w-35">
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
              </div>
            </div>
          </section>

          {/* 文件传输设置 */}
          <TransferSettingsSection />

          {/* MCP Server */}
          <McpSection />
        </div>
      </div>
    </main>
  );
}
