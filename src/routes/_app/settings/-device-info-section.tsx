import { useState, useCallback, useEffect, type ComponentType } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg, type MacroMessageDescriptor } from "@lingui/core/macro";
import { toast } from "sonner";
import { Copy, Check, Pencil, Cpu, Activity, Zap, ShieldCheck } from "lucide-react";
import {
  platform,
  arch,
  version,
  type as osType,
  hostname,
} from "@tauri-apps/plugin-os";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useSecretStore } from "@/stores/secret-store";
import { useNetworkStore } from "@/stores/network-store";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { applyDeviceName } from "@/lib/device-name";
import { getErrorMessage } from "@/lib/errors";

/** 截断 PeerId，显示前8位...后4位 */
function truncatePeerId(id: string): string {
  if (id.length <= 16) return id;
  return `${id.slice(0, 8)}...${id.slice(-4)}`;
}

/** 平台显示名称 */
function getPlatformLabel(p: string): string {
  const map: Record<string, string> = {
    windows: "Windows",
    macos: "macOS",
    linux: "Linux",
    android: "Android",
    ios: "iOS",
  };
  return map[p] ?? p;
}

/** 底部指标项定义 */
interface StatItem {
  icon: ComponentType<{ className?: string }>;
  label: MacroMessageDescriptor;
  value: React.ReactNode;
}

export function DeviceInfoSection() {
  const { t } = useLingui();
  const deviceName = usePreferencesStore((s) => s.deviceName);
  const deviceId = useSecretStore((s) => s.deviceId);
  const pairedCount = useSecretStore((s) => s.pairedDevices.length);
  const nodeStatus = useNetworkStore((s) => s.status);
  const networkStatus = useNetworkStore((s) => s.networkStatus);

  const [systemHostname, setSystemHostname] = useState("");
  const [editing, setEditing] = useState(false);
  const [nameInput, setNameInput] = useState(deviceName);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    hostname().then((name) => setSystemHostname(name ?? ""));
  }, []);

  useEffect(() => {
    setNameInput(deviceName);
  }, [deviceName]);

  const displayName = deviceName || systemHostname || "SwarmDrop";
  const avatarInitials = displayName.slice(0, 2).toUpperCase();
  const currentPlatform = platform();
  const currentArch = arch();
  const currentOsVersion = version();
  const currentOsType = osType();
  const DeviceIcon = getDeviceIcon(currentOsType);

  const osLabel = `${getPlatformLabel(currentPlatform)} ${currentOsVersion} · ${currentArch}`;

  const handleSaveName = useCallback(async () => {
    const trimmed = nameInput.trim();
    if (trimmed && trimmed !== deviceName) {
      try {
        await applyDeviceName(trimmed);
        toast.success(t`设备名称已更新`);
      } catch (err) {
        toast.error(getErrorMessage(err));
        return;
      }
    }
    setEditing(false);
  }, [nameInput, deviceName, t]);

  const handleCopyPeerId = useCallback(() => {
    if (!deviceId) return;
    navigator.clipboard.writeText(deviceId).then(() => {
      setCopied(true);
      toast.success(t`已复制到剪贴板`);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [deviceId, t]);

  const connectedPeers = networkStatus?.connectedPeers ?? 0;
  const natStatus = networkStatus?.natStatus ?? "unknown";
  const isOnline = nodeStatus === "running";

  const stats: StatItem[] = [
    {
      icon: Zap,
      label: msg`已连节点`,
      value: (
        <span className="text-xl font-bold tracking-tight text-foreground sm:text-2xl">
          {connectedPeers}
        </span>
      ),
    },
    {
      icon: ShieldCheck,
      label: msg`配对设备`,
      value: (
        <span className="text-xl font-bold tracking-tight text-foreground sm:text-2xl">
          {pairedCount}
        </span>
      ),
    },
    {
      icon: Activity,
      label: msg`NAT 状态`,
      value: (
        <Badge
          variant="outline"
          className={`rounded-md border-transparent px-2 py-0.5 text-[11px] font-medium sm:px-3 sm:py-1 sm:text-xs ${
            natStatus === "public"
              ? "bg-blue-50 text-blue-600 dark:bg-blue-900/30 dark:text-blue-400"
              : "bg-muted text-muted-foreground"
          }`}
        >
          {natStatus === "public" ? t`映射成功` : t`未知`}
        </Badge>
      ),
    },
  ];

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold text-foreground">
        <Trans>设备信息</Trans>
      </h2>
      <div className="glass-card overflow-hidden rounded-xl">
        {/* 上半部分：身份识别区 */}
        <div className="flex items-center gap-4 p-4 sm:gap-5 sm:p-6">
          {/* 头像区域 */}
          <div className="relative shrink-0">
            <div
              className={`absolute -left-1 -top-1 z-10 size-3.5 rounded-full border-2 border-background ${
                isOnline ? "bg-green-500" : "bg-slate-300"
              }`}
            />
            <div className="flex size-14 items-center justify-center rounded-2xl bg-blue-50/80 dark:bg-blue-900/20 sm:size-16">
              <span className="text-xl font-bold tracking-tight text-blue-600 dark:text-blue-400 sm:text-2xl">
                {avatarInitials}
              </span>
            </div>
            <div className="absolute -bottom-1 -right-1 flex size-5 items-center justify-center rounded-lg border border-border bg-background shadow-sm sm:size-6">
              <DeviceIcon className="size-3 text-muted-foreground sm:size-3.5" />
            </div>
          </div>

          {/* 设备信息区 */}
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            {/* 设备名称 */}
            <div className="group flex items-center gap-2">
              {editing ? (
                <Input
                  value={nameInput}
                  onChange={(e) => setNameInput(e.target.value)}
                  onBlur={handleSaveName}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleSaveName();
                    if (e.key === "Escape") {
                      setNameInput(deviceName);
                      setEditing(false);
                    }
                  }}
                  className="h-7 w-full max-w-50 px-1 py-0 text-base font-bold sm:text-lg"
                  autoFocus
                />
              ) : (
                <>
                  <h3 className="truncate text-base font-bold text-foreground sm:text-lg">
                    {displayName}
                  </h3>
                  {isOnline && (
                    <span className="shrink-0 rounded-full border border-green-200 bg-green-50 px-1.5 py-0.5 text-[10px] font-medium text-green-600 dark:border-green-900/50 dark:bg-green-900/20 dark:text-green-400 sm:px-2 sm:text-[11px]">
                      {t`在线`}
                    </span>
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-6 shrink-0 opacity-0 transition-opacity group-hover:opacity-100"
                    onClick={() => {
                      setNameInput(deviceName || systemHostname);
                      setEditing(true);
                    }}
                  >
                    <Pencil className="size-3 text-muted-foreground" />
                  </Button>
                </>
              )}
            </div>

            {/* 系统版本 */}
            <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
              <Cpu className="size-3.5 shrink-0" />
              <span className="truncate">{osLabel}</span>
            </div>

            {/* Peer ID */}
            <div
              className="group flex cursor-pointer items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-primary"
              onClick={handleCopyPeerId}
            >
              <Activity className="size-3.5 shrink-0" />
              <span className="truncate font-mono text-[13px]">
                {truncatePeerId(deviceId ?? "")}
              </span>
              {copied ? (
                <Check className="size-3.5 shrink-0 text-green-500" />
              ) : (
                <Copy className="size-3.5 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
              )}
            </div>
          </div>
        </div>

        {/* 下半部分：网络指标区 */}
        <div className="grid grid-cols-3 divide-x divide-border border-t border-border">
          {stats.map((stat, i) => (
            <div
              key={i}
              className="flex flex-col items-center justify-center px-2 py-4 transition-colors hover:bg-muted/30 sm:py-5"
            >
              <div className="mb-1.5 flex items-center gap-1 text-muted-foreground sm:gap-1.5">
                <stat.icon className="size-3.5 text-slate-400" />
                <span className="text-[11px] font-medium sm:text-xs">
                  {t(stat.label)}
                </span>
              </div>
              {stat.value}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
