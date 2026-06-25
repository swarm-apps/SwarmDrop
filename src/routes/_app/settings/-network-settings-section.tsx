/**
 * NetworkSettingsSection
 * 设置页「网络」区域 — P2P 网络相关设置
 */

import { Trans } from "@lingui/react/macro";
import { Switch } from "@/components/ui/switch";
import { usePreferencesStore } from "@/stores/preferences-store";

export function NetworkSettingsSection() {
  const autoStart = usePreferencesStore((state) => state.autoStart);
  const setAutoStart = usePreferencesStore((state) => state.setAutoStart);

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold text-foreground">
        <Trans>网络</Trans>
      </h2>
      <div className="glass-card rounded-lg">
        {/* 自动启动节点 */}
        <div className="flex items-center justify-between p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>自动启动节点</Trans>
            </span>
            <span className="text-xs text-muted-foreground">
              <Trans>解锁后自动启动 P2P 网络节点</Trans>
            </span>
          </div>
          <Switch checked={autoStart} onCheckedChange={setAutoStart} />
        </div>
      </div>
    </section>
  );
}
