/**
 * NetworkSettingsSection
 * 设置页「网络」区域 — P2P 网络相关设置
 */

import { useCallback, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg } from "@lingui/core/macro";
import { RotateCw } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { usePreferencesStore, type DiscoveryMode } from "@/stores/preferences-store";
import { useNetworkStore } from "@/stores/network-store";
import { toast } from "sonner";

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
  const nodeStatus = useNetworkStore((state) => state.status);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [restarting, setRestarting] = useState(false);

  function markRestartNeeded() {
    if (nodeStatus === "running") {
      setNeedsRestart(true);
    }
  }

  const handleRestart = useCallback(async () => {
    setRestarting(true);
    try {
      const { stopNetwork, startNetwork } = useNetworkStore.getState();
      await stopNetwork();
      const ok = await startNetwork();
      if (!ok) {
        setNeedsRestart(true);
        return;
      }
      setNeedsRestart(false);
      toast.success(t(msg`节点已重启`));
    } catch {
      toast.error(t(msg`重启节点失败`));
    } finally {
      setRestarting(false);
    }
  }, [t]);

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold text-foreground">
        <Trans>网络</Trans>
      </h2>
      <div className="glass-card rounded-lg">
        {/* 自动启动节点 */}
        <div className="flex items-center justify-between border-b border-border p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>自动启动节点</Trans>
            </span>
            <span className="text-xs text-muted-foreground">
              <Trans>解锁后自动启动 P2P 网络节点</Trans>
            </span>
          </div>
          <Switch
            aria-label={t(msg`自动启动节点`)}
            checked={autoStart}
            onCheckedChange={setAutoStart}
          />
        </div>

        <div className="flex items-center justify-between border-b border-border p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>发现模式</Trans>
            </span>
            <span className="text-xs text-muted-foreground">
              <Trans>控制是否连接公网引导节点</Trans>
            </span>
          </div>
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
        </div>

        <div className="flex items-center justify-between border-b border-border p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>自动发现局域网协助节点</Trans>
            </span>
            <span className="text-xs text-muted-foreground">
              <Trans>使用同网段已开启协助能力的桌面端</Trans>
            </span>
          </div>
          <Switch
            aria-label={t(msg`自动发现局域网协助节点`)}
            checked={autoDiscoverLanHelpers}
            onCheckedChange={(enabled) => {
              setAutoDiscoverLanHelpers(enabled);
              markRestartNeeded();
            }}
          />
        </div>

        <div className="flex items-center justify-between p-4">
          <div className="flex flex-col gap-0.5">
            <span className="text-sm font-medium text-foreground">
              <Trans>本设备作为局域网协助节点</Trans>
            </span>
            <span className="text-xs text-muted-foreground">
              <Trans>为同网段设备提供受限发现与中继能力</Trans>
            </span>
          </div>
          <Switch
            aria-label={t(msg`本设备作为局域网协助节点`)}
            checked={provideLanHelper}
            onCheckedChange={(enabled) => {
              setProvideLanHelper(enabled);
              markRestartNeeded();
            }}
          />
        </div>
      </div>

      {needsRestart && nodeStatus === "running" && (
        <div className="flex items-center justify-between rounded-lg border border-amber-200 bg-amber-50 p-3 dark:border-amber-900 dark:bg-amber-950">
          <span className="text-xs text-amber-800 dark:text-amber-200">
            <Trans>网络发现设置已变更，需重启节点生效</Trans>
          </span>
          <Button
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            onClick={handleRestart}
            disabled={restarting}
          >
            <RotateCw className={`mr-1 size-3 ${restarting ? "animate-spin" : ""}`} />
            {restarting ? <Trans>重启中...</Trans> : <Trans>重启节点</Trans>}
          </Button>
        </div>
      )}
    </section>
  );
}
