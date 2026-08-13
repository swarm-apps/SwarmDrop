/**
 * NetworkSettingsSection
 * 设置页「网络」区域 — P2P 网络相关设置
 */

import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { Network } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { usePreferencesStore } from "@/stores/preferences-store";
import { LanHelperAddress } from "@/components/network/lan-helper-address";
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
  const autoDiscoverLanHelpers = usePreferencesStore((state) => state.autoDiscoverLanHelpers);
  const setAutoDiscoverLanHelpers = usePreferencesStore((state) => state.setAutoDiscoverLanHelpers);
  const provideLanHelper = usePreferencesStore((state) => state.provideLanHelper);
  const setProvideLanHelper = usePreferencesStore((state) => state.setProvideLanHelper);
  const publicReachability = usePreferencesStore((state) => state.publicReachability);
  const setPublicReachability = usePreferencesStore((state) => state.setPublicReachability);
  const {
    restarting,
    markRestartNeeded,
    restart,
    showBanner,
    activeTransferCount,
  } = useNodeRestart();

  return (
    <SettingsSection title={<Trans>网络</Trans>} icon={Network} fill>
      <SettingsCard fill>
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
          title={<Trans>公网可达性</Trans>}
          description={
            <Trans>允许通过公网中继被跨网设备访问；关闭后仅局域网可达，跨网设备找不到你</Trans>
          }
          action={
            <Switch
              aria-label={t(msg`公网可达性`)}
              checked={publicReachability}
              onCheckedChange={(enabled) => {
                setPublicReachability(enabled);
                markRestartNeeded();
              }}
            />
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

        {/* 重启提示**贴着产生它的那些开关**，不再吊在页面底部。
            引导节点已不在这条路径上——增删当场生效，不需要重启（见 `-bootstrap-nodes-section`），
            所以这里剩下的全是真的要重启的开关。 */}
        {showBanner && (
          <div className="border-b border-border/60 p-4 last:border-b-0">
            <NodeRestartBanner
              activeTransferCount={activeTransferCount}
              message={<Trans>网络发现设置已变更，需重启节点生效</Trans>}
              restarting={restarting}
              onRestart={restart}
            />
          </div>
        )}
      </SettingsCard>

      <LanHelperAddress />
    </SettingsSection>
  );
}
