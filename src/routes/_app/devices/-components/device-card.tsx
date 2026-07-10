import { useState, type ComponentType } from "react";
import { cn } from "@/lib/utils";
import { deviceDisplayName } from "@/lib/device-name";
import { formatLatency } from "@/lib/format";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import {
  Link,
  MoreHorizontal,
  RadioTower,
  Send,
  Settings2,
  Unlink,
  Wifi,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { useLingui } from "@lingui/react/macro";
import { Trans } from "@lingui/react/macro";
import type {
  Device,
  ConnectionType,
  DeviceReceivePolicy,
  DeviceTrustLevel,
} from "@/lib/bindings";
import { TrustPolicyDialog, trustConfig } from "./trust-policy-dialog";

const statusTone = {
  unpaired:
    "bg-primary/10 text-brand ring-primary/10 dark:bg-primary/12 dark:ring-primary/15",
};

const connectionConfig: Record<
  ConnectionType,
  {
    icon: ComponentType<{ className?: string }>;
    label: MessageDescriptor;
    bgColor: string;
    textColor: string;
  }
> = {
  lan: {
    icon: Wifi,
    label: msg`局域网`,
    bgColor: "bg-green-100",
    textColor: "text-green-600",
  },
  dcutr: {
    icon: Zap,
    label: msg`打洞`,
    bgColor: "bg-sky-100",
    textColor: "text-sky-600",
  },
  relay: {
    icon: RadioTower,
    label: msg`中继`,
    bgColor: "bg-amber-100",
    textColor: "text-amber-600",
  },
};

interface DeviceCardProps {
  device: Device;
  onSend?: (device: Device) => void;
  onConnect?: (device: Device) => void;
  onUnpair?: (device: Device) => void;
  onUpdatePolicy?: (
    device: Device,
    trustLevel: DeviceTrustLevel,
    receivePolicy: DeviceReceivePolicy,
  ) => Promise<void>;
}

export function DeviceCard({
  device,
  onSend,
  onConnect,
  onUnpair,
  onUpdatePolicy,
}: DeviceCardProps) {
  const { t } = useLingui();
  const DeviceIcon = getDeviceIcon(device.os);
  const isOnline = device.status === "online";
  const connConfig = device.connection ? connectionConfig[device.connection] : null;

  const [unpairOpen, setUnpairOpen] = useState(false);
  const [policyOpen, setPolicyOpen] = useState(false);

  // 桌面端纵向卡片样式
  // 整张卡可点击:已配对+在线点击 = 发送;未配对点击 = 连接
  const handleCardClick = () => {
    if (device.isPaired) {
      if (isOnline) onSend?.(device);
    } else {
      onConnect?.(device);
    }
  };

  const isInteractive = device.isPaired ? isOnline : !!onConnect;

  return (
    <>
      <div
        role={isInteractive ? "button" : undefined}
        tabIndex={isInteractive ? 0 : -1}
        data-testid="device-card"
        data-peer-id={device.peerId}
        data-device-status={device.status}
        data-device-paired={device.isPaired ? "true" : "false"}
        onClick={isInteractive ? handleCardClick : undefined}
        onKeyDown={(e) => {
          if (e.currentTarget !== e.target) return;
          if (isInteractive && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            handleCardClick();
          }
        }}
        className={cn(
          "group relative flex min-h-[132px] flex-col gap-2.5 overflow-hidden rounded-[22px] p-3.5 transition-[border-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)]",
          device.isPaired && isOnline ? "glass-accent" : "glass-card",
          isInteractive
            ? "cursor-pointer hover:border-primary/25 hover:shadow-[0_18px_42px_rgba(219,163,65,0.10)] active:scale-[0.99]"
            : "opacity-72",
        )}
      >
        <div className="pointer-events-none absolute inset-x-4 top-0 h-px bg-white/90 dark:bg-white/15" />
        {/* Header */}
        <div className="flex items-center gap-2.5">
          <div
            className={cn(
              "glass-control flex size-11 shrink-0 items-center justify-center rounded-[15px] transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:scale-105",
              isOnline
                ? "text-brand"
                : "text-muted-foreground",
            )}
          >
            <DeviceIcon
              className={cn(
                "size-5",
                isOnline ? "text-brand" : "text-muted-foreground"
              )}
            />
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="truncate text-sm font-medium text-foreground">
              {deviceDisplayName(device)}
            </span>
            <div className="flex items-center gap-1">
              {device.isPaired ? (
                <>
                  <span
                    className={cn(
                      "size-1.5 rounded-full",
                      isOnline ? "bg-green-500" : "bg-muted-foreground"
                    )}
                  />
                  <span
                    className={cn(
                      "text-[11px]",
                      isOnline ? "text-green-500" : "text-muted-foreground"
                    )}
                  >
                    {isOnline ? <Trans>在线</Trans> : <Trans>离线</Trans>}
                  </span>
                </>
              ) : (
                <span className="text-[11px] text-muted-foreground">
                  <Trans>未配对</Trans>
                </span>
              )}
            </div>
          </div>
          {/* More Menu (paired only) */}
          {device.isPaired && onUnpair && (
            <DeviceActionMenu
              onPolicyClick={
                onUpdatePolicy ? () => setPolicyOpen(true) : undefined
              }
              onUnpairClick={() => setUnpairOpen(true)}
              label={t`设备操作`}
            />
          )}
        </div>

        {/* Footer */}
        <div className="mt-auto flex items-center justify-between gap-2">
          {/* Connection Badge */}
          {connConfig && device.latency != null ? (
            <div
              className={cn(
                "flex items-center gap-1 rounded-full px-2.5 py-1 ring-1 ring-black/[0.03]",
                connConfig.bgColor
              )}
            >
              <connConfig.icon className={cn("size-2.5", connConfig.textColor)} />
              <span className={cn("text-[10px] font-medium", connConfig.textColor)}>
                {t(connConfig.label)}
              </span>
              {formatLatency(device.latency) && (
                <span className={cn("text-[10px] font-medium", connConfig.textColor)}>
                  {formatLatency(device.latency)}
                </span>
              )}
            </div>
          ) : (
            <TrustBadge device={device} />
          )}

          {/* Action Button */}
          {device.isPaired ? (
            <Button
              size="sm"
              variant={isOnline ? "default" : "outline"}
              disabled={!isOnline}
              data-testid="device-send-action"
              onClick={(e) => {
                e.stopPropagation();
                onSend?.(device);
              }}
              className={cn(
                "h-auto shrink-0 gap-1.5 rounded-full px-3 py-1.5 text-xs transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97]",
                isOnline
                  ? "shadow-[0_8px_18px_rgba(219,163,65,0.18)]"
                  : "glass-control border-transparent text-muted-foreground"
              )}
            >
              <Send className="size-3.5" />
              <Trans>发送</Trans>
            </Button>
          ) : (
            <Button
              size="sm"
              variant="outline"
              data-testid="device-connect-action"
              onClick={(e) => {
                e.stopPropagation();
                onConnect?.(device);
              }}
              className="glass-control h-auto shrink-0 gap-1.5 rounded-full border-transparent px-3 py-1.5 text-xs text-brand transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97]"
            >
              <Link className="size-3.5" />
              <Trans>连接</Trans>
            </Button>
          )}
        </div>
      </div>

      {/* 取消配对确认弹窗 */}
      <UnpairAlertDialog
        open={unpairOpen}
        onOpenChange={setUnpairOpen}
        deviceName={deviceDisplayName(device)}
        onConfirm={() => onUnpair?.(device)}
      />
      {onUpdatePolicy && (
        <TrustPolicyDialog
          open={policyOpen}
          onOpenChange={setPolicyOpen}
          device={device}
          onSubmit={onUpdatePolicy}
        />
      )}
    </>
  );
}

function DeviceActionMenu({
  onPolicyClick,
  onUnpairClick,
  label,
}: {
  onPolicyClick?: () => void;
  onUnpairClick: () => void;
  label: string;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          onClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => event.stopPropagation()}
          data-testid="device-actions-menu"
          aria-label={label}
          title={label}
          className="glass-control flex size-8 items-center justify-center rounded-full text-muted-foreground transition-[color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] hover:text-foreground active:scale-[0.96]"
        >
          <MoreHorizontal className="size-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="right"
        align="start"
        sideOffset={4}
        className="glass-card min-w-[112px] rounded-[14px] p-1"
        onClick={(event) => event.stopPropagation()}
      >
        {onPolicyClick && (
          <>
            <DropdownMenuItem
              onSelect={(event) => {
                event.stopPropagation();
                onPolicyClick();
              }}
              data-testid="device-policy-menu-action"
              className="h-8 rounded-[10px] px-2.5 text-xs font-medium"
            >
              <Settings2 className="size-3.5" />
              <Trans>信任策略</Trans>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem
          variant="destructive"
          data-testid="device-unpair-menu-action"
          onSelect={(event) => {
            event.stopPropagation();
            onUnpairClick();
          }}
          className="h-8 rounded-[10px] px-2.5 text-xs font-medium text-destructive/90 focus:bg-destructive/10 focus:text-destructive dark:focus:bg-destructive/15"
        >
          <Unlink className="size-3.5" />
          <Trans>取消配对</Trans>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function TrustBadge({ device }: { device: Device }) {
  if (!device.isPaired) {
    return (
      <span
        className={cn(
          "rounded-full px-2.5 py-1 text-[10px] font-medium ring-1",
          statusTone.unpaired,
        )}
      >
        <Trans>未配对</Trans>
      </span>
    );
  }

  const trust = trustConfig(device.trustLevel ?? "collaborator");
  const Icon = trust.icon;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-[10px] font-medium ring-1",
        trust.className,
      )}
    >
      <Icon className="size-3" />
      {trust.label}
      {device.trustConfirmed === false && (
        <span className="text-muted-foreground">· <Trans>待确认</Trans></span>
      )}
    </span>
  );
}

/** 取消配对确认弹窗 */
function UnpairAlertDialog({
  open,
  onOpenChange,
  deviceName,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  deviceName: string;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Trans>取消配对</Trans>
          </AlertDialogTitle>
          <AlertDialogDescription>
            <Trans>
              确定要取消与「{deviceName}」的配对吗？取消后需要重新配对才能传输文件。
            </Trans>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel data-testid="device-unpair-cancel-action">
            <Trans>取消</Trans>
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            data-testid="device-unpair-confirm-action"
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            <Trans>确认取消配对</Trans>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
