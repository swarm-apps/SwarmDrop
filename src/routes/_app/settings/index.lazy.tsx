/**
 * Settings Page (Lazy)
 * 设置页面 - 懒加载组件
 *
 * 布局：固定双列（左=设备/外观/传输，右=网络/引导节点/MCP），关于沉底跨两列。
 * 窄屏（< md）自动退化为单列。标题由 AppTopBar 面包屑承担。
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
        <div className="mx-auto max-w-[960px]">
          <div className="settings-board">
            {/* 左列：你的设备 + 怎么用 */}
            <div className="settings-column">
              <div id="device" className="scroll-mt-6">
                <DeviceInfoSection />
              </div>
              <div id="appearance" className="scroll-mt-6">
                <AppearanceSection />
              </div>

              <div id="general" className="scroll-mt-6">
                <GeneralSection />
              </div>

              <div id="transfer" className="scroll-mt-6">
                <TransferSettingsSection />
              </div>
            </div>

            {/* 右列：怎么连 + 进阶 */}
            <div className="settings-column">
              <div id="network" className="scroll-mt-6">
                <NetworkSettingsSection />
              </div>
              <div id="bootstrap" className="scroll-mt-6">
                <BootstrapNodesSection />
              </div>
              <div id="mcp" className="scroll-mt-6">
                <McpSection />
              </div>
            </div>
          </div>

          {/* 关于沉底，跨两列 */}
          <div id="about" className="mt-5 scroll-mt-6">
            <AboutSection />
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

/** 外观设置：主题 + 语言 */
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
  );
}
