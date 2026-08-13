import { useState, useCallback, useEffect, type ComponentType } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { msg, type MacroMessageDescriptor } from "@lingui/core/macro";
import { toast } from "sonner";
import {
  Activity,
  Check,
  Copy,
  Cpu,
  MonitorSmartphone,
  Pencil,
  ShieldCheck,
  Zap,
} from "lucide-react";
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
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { useNodeHealth } from "@/hooks/use-node-health";
import { usePairedOnlineCount } from "@/hooks/use-paired-online-count";
import { applyDeviceName, DEVICE_NAME_MAX_CHARS } from "@/lib/device-name";
import {
  resolveNodePresentation,
  TONE_BADGE,
  TONE_DOT,
} from "@/lib/node-status";
import { copyText } from "@/lib/clipboard";
import { getErrorMessage } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { SettingsCard, SettingsSection } from "./-settings-primitives";

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
  // 状态判据只有一个来源：`summarizeNodeHealth`。此前这里自造了一份
  // `nodeStatus === "running"` 的「在线」，于是全部中继 Failed 时顶栏写着「连不上」、
  // 同一屏的设备卡却绿着说「在线」——同一件事在一个窗口里给出两个相反的结论。
  const { summary, lifecycle, networkStatus } = useNodeHealth();
  const presentation = resolveNodePresentation(lifecycle, summary);

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
        // 成功即「已落盘 + 已连接的对端已收到新名字」，没有中间态要分开汇报：失败一定
        // 从后端的 AppResult 抛上来，落盘失败时网络那一步压根不会执行。
        await applyDeviceName(trimmed);
        toast.success(t`设备名称已更新`);
      } catch (err) {
        toast.error(getErrorMessage(err));
        return;
      }
    }
    setEditing(false);
  }, [nameInput, deviceName, t]);

  // 失败必须显式报出来：`copyText` reject 时若不接 catch，Check 图标不亮、toast 也不弹，
  // 表现和「没点到」完全一样（见 theme-and-styling.md 的静默 catch 教训）。
  const handleCopyPeerId = useCallback(() => {
    if (!deviceId) return;
    copyText(deviceId).then(
      () => {
        setCopied(true);
        toast.success(t`已复制到剪贴板`);
        setTimeout(() => setCopied(false), 2000);
      },
      () => toast.error(t`复制失败`),
    );
  }, [deviceId, t]);

  // 与节点状态面的「在线 M」同源（那条注释解释了为什么不能用 `connectedPeers`）。
  const onlineDeviceCount = usePairedOnlineCount();
  const natStatus = networkStatus?.natStatus ?? "unknown";

  const stats: StatItem[] = [
    {
      icon: Zap,
      // 词表（DESIGN.md「Network vocabulary is cross-platform」）钉死的正字，
      // 「已连节点」是那张表里逐字点名的废弃拼法。
      label: msg`已连接设备`,
      value: (
        <span className="text-xl font-bold tracking-tight text-foreground sm:text-2xl">
          {onlineDeviceCount}
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
              ? "bg-primary/10 text-brand"
              : "bg-muted text-muted-foreground"
          }`}
        >
          {natStatus === "public" ? t`映射成功` : t`未知`}
        </Badge>
      ),
    },
  ];

  return (
    <SettingsSection title={<Trans>设备信息</Trans>} icon={MonitorSmartphone}>
      <SettingsCard>
        <div className="flex flex-col lg:flex-row lg:items-stretch">
          {/* 身份识别区 */}
          <div className="flex flex-1 items-center gap-4 p-4 sm:gap-5 sm:p-5">
            {/* 头像区域 */}
            <div className="relative shrink-0">
              <div
                className={cn(
                  "absolute -left-1 -top-1 z-10 size-3.5 rounded-full border-2 border-background",
                  TONE_DOT[presentation.tone],
                )}
              />
              <div className="flex size-14 items-center justify-center rounded-2xl bg-primary/10 sm:size-16">
                <span className="text-xl font-bold tracking-tight text-brand sm:text-2xl">
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
                    maxLength={DEVICE_NAME_MAX_CHARS}
                  />
                ) : (
                  <>
                    <h3 className="truncate text-base font-bold text-foreground sm:text-lg">
                      {displayName}
                    </h3>
                    {/* 光有色点不满足契约，状态**词**必须一起出现；词与色都取自同一份
                        判据，跟顶栏 pill 说的是同一句话。 */}
                    <span
                      className={cn(
                        "shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium sm:px-2 sm:text-[11px]",
                        TONE_BADGE[presentation.tone],
                      )}
                    >
                      {t(presentation.word)}
                    </span>
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
                className="group flex cursor-pointer items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-brand"
                onClick={handleCopyPeerId}
              >
                <Activity className="size-3.5 shrink-0" />
                <span className="truncate font-mono text-[13px]">
                  {truncatePeerId(deviceId ?? "")}
                </span>
                {copied ? (
                  <Check className="size-3.5 shrink-0 text-success-ink" />
                ) : (
                  <Copy className="size-3.5 shrink-0 opacity-0 transition-opacity group-hover:opacity-100" />
                )}
              </div>
            </div>
          </div>

          {/* 网络指标区：窄屏沉底，宽屏移到右侧并用左分隔 */}
          <div className="grid grid-cols-3 divide-x divide-border/60 border-t border-border/60 lg:w-[360px] lg:shrink-0 lg:border-l lg:border-t-0">
            {stats.map((stat, i) => (
              <div
                key={i}
                className="flex flex-col items-center justify-center px-2 py-4 transition-colors hover:bg-muted/30"
              >
                <div className="mb-1.5 flex items-center gap-1 text-muted-foreground sm:gap-1.5">
                  <stat.icon className="size-3.5 text-muted-foreground" />
                  <span className="text-[11px] font-medium sm:text-xs">
                    {t(stat.label)}
                  </span>
                </div>
                {stat.value}
              </div>
            ))}
          </div>
        </div>
      </SettingsCard>
    </SettingsSection>
  );
}
