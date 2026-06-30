/**
 * NetworkSettingsSection
 * 设置页「网络」区域 — P2P 网络相关设置
 */

import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { Network } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { usePreferencesStore, type DiscoveryMode } from "@/stores/preferences-store";
import { useNodeRestart } from "@/hooks/use-node-restart";
import {
  NodeRestartBanner,
  SettingsCard,
  SettingsRow,
  SettingsSection,
} from "./-settings-primitives";

export function NetworkSettingsSection() {
  const { t } = useLingui();
  const autoStart = usePreferencesStore((state) => state.autoStart);
  const setAutoStart = usePreferencesStore((state) => state.setAutoStart);
  const discoveryMode = usePreferencesStore((state) => state.discoveryMode);
  const setDiscoveryMode = usePreferencesStore((state) => state.setDiscoveryMode);
  const autoDiscoverLanHelpers = usePreferencesStore((state) => state.autoDiscoverLanHelpers);
  const setAutoDiscoverLanHelpers = usePreferencesStore((state) => state.setAutoDiscoverLanHelpers);
  const provideLanHelper = usePreferencesStore((state) => state.provideLanHelper);
  const setProvideLanHelper = usePreferencesStore((state) => state.setProvideLanHelper);
  const { restarting, markRestartNeeded, restart, showBanner } = useNodeRestart();

  return (
    <SettingsSection title={<Trans>网络</Trans>} icon={Network}>
      <SettingsCard>
        <SettingsRow
          title={<Trans>自动启动节点</Trans>}
          description={<Trans>解锁后自动启动 P2P 网络节点</Trans>}
          action={
            <Switch
              aria-label={t(msg`自动启动节点`)}
              checked={autoStart}
              onCheckedChange={setAutoStart}
            />
          }
        />

        <SettingsRow
          title={<Trans>发现模式</Trans>}
          description={<Trans>控制是否连接公网引导节点</Trans>}
          action={
            <Select
              value={discoveryMode}
              onValueChange={(value) => {
                setDiscoveryMode(value as DiscoveryMode);
                markRestartNeeded();
              }}
            >
              <SelectTrigger aria-label={t(msg`发现模式`)} className="w-34 sm:w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">{t(msg`自动`)}</SelectItem>
                <SelectItem value="lanOnly">{t(msg`仅局域网`)}</SelectItem>
              </SelectContent>
            </Select>
          }
        />

        <SettingsRow
          title={<Trans>自动发现局域网协助节点</Trans>}
          description={<Trans>使用同网段已开启协助能力的桌面端</Trans>}
          action={
            <Switch
              aria-label={t(msg`自动发现局域网协助节点`)}
              checked={autoDiscoverLanHelpers}
              onCheckedChange={(enabled) => {
                setAutoDiscoverLanHelpers(enabled);
                markRestartNeeded();
              }}
            />
          }
        />

        <SettingsRow
          title={<Trans>本设备作为局域网协助节点</Trans>}
          description={<Trans>为同网段设备提供受限发现与中继能力</Trans>}
          action={
            <Switch
              aria-label={t(msg`本设备作为局域网协助节点`)}
              checked={provideLanHelper}
              onCheckedChange={(enabled) => {
                setProvideLanHelper(enabled);
                markRestartNeeded();
              }}
            />
          }
        />
      </SettingsCard>

      {showBanner && (
        <NodeRestartBanner
          message={<Trans>网络发现设置已变更，需重启节点生效</Trans>}
          restarting={restarting}
          onRestart={restart}
        />
      )}
    </SettingsSection>
  );
}
