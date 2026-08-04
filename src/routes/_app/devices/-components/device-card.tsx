import { useState } from "react";
import { canSendToDevice, normalizeTrustLevel } from "@swarmdrop/shared-view";
import { cn } from "@/lib/utils";
import { deviceDisplayName } from "@/lib/device-name";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import {
  Link,
  MoreHorizontal,
  Send,
  Settings2,
  Tags,
  Unlink,
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
import { useLingui } from "@lingui/react/macro";
import { Trans } from "@lingui/react/macro";
import type {
  Device,
  DeviceReceivePolicy,
  DeviceTrustLevel,
} from "@/lib/bindings";
import { ConnectionBadge } from "./connection-badge";
import { TrustPolicyDialog, trustConfig } from "./trust-policy-dialog";

const statusTone = {
  unpaired:
    "bg-primary/10 text-brand ring-primary/10 dark:bg-primary/12 dark:ring-primary/15",
};

interface DeviceCardProps {
  device: Device;
  displayName?: string;
  groupNames?: string[];
  identityHint?: string;
  showIdentityHint?: boolean;
  onSend?: (device: Device) => void;
  onConnect?: (device: Device) => void;
  onUnpair?: (device: Device) => void;
  onOrganize?: (device: Device) => void;
  onUpdatePolicy?: (
    device: Device,
    trustLevel: DeviceTrustLevel,
    receivePolicy: DeviceReceivePolicy,
  ) => Promise<void>;
}

export function DeviceCard({
  device,
  displayName = deviceDisplayName(device),
  groupNames = [],
  identityHint,
  showIdentityHint = false,
  onSend,
  onConnect,
  onUnpair,
  onOrganize,
  onUpdatePolicy,
}: DeviceCardProps) {
  const { t } = useLingui();
  const DeviceIcon = getDeviceIcon(device.os);
  const isOnline = device.status === "online";
  /**
   * 能不能发。**必须走共享包，不能只判 `isOnline`**：此前整卡点击与发送按钮的判据里都
   * 没有信任级别，于是一台**在线的已阻止设备**在桌面上整卡可点、按钮高亮，点下去才由
   * 内核拒绝——而「阻止」正是用户明确表态过不要跟它来往的那一档。
   * Web 端一直用的这个函数；DESIGN.md 跨端清单也点名要求信任归一化来自 `shared-view`
   * 而不是各端本地副本。
   */
  const canSend = canSendToDevice(device);

  const [unpairOpen, setUnpairOpen] = useState(false);
  const [policyOpen, setPolicyOpen] = useState(false);

  // 桌面端纵向卡片样式
  // 整张卡可点击:已配对+在线点击 = 发送;未配对点击 = 连接
  const handleCardClick = () => {
    if (device.isPaired) {
      if (canSend) onSend?.(device);
    } else {
      onConnect?.(device);
    }
  };

  const isInteractive = device.isPaired ? canSend : !!onConnect;

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
          // 强调层跟着**能不能用**走，不是「在不在线」。一台在线的已阻止设备如果照样
          // 高亮，卡片在说「来点我」，而按钮禁用、信任徽标写着「已阻止」——同一张卡上
          // 视觉与语义对着干。
          device.isPaired && canSend ? "glass-accent" : "glass-card",
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
              {displayName}
            </span>
            {/* 在线态只对已配对设备有意义，也只在这里说一次。
                未配对时这一整行不渲染——「未配对」由下方恒在场的 `TrustBadge` 说，
                两处都摆等于在一张 132px 的卡里把同一句话说两遍（同按钮不重复禁用
                原因那条，见下方注释）。 */}
            {device.isPaired && (
              <div className="flex items-center gap-1">
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
              </div>
            )}
            {(showIdentityHint || groupNames.length > 0) && (
              <p className="truncate text-[10px] text-muted-foreground">
                {[
                  ...groupNames,
                  showIdentityHint ? identityHint : undefined,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </p>
            )}
          </div>
          {/* More Menu (paired only) */}
          {device.isPaired && onUnpair && (
            <DeviceActionMenu
              onOrganizeClick={onOrganize ? () => onOrganize(device) : undefined}
              onPolicyClick={
                onUpdatePolicy ? () => setPolicyOpen(true) : undefined
              }
              onUnpairClick={() => setUnpairOpen(true)}
              label={t`设备操作`}
            />
          )}
        </div>

        {/* Footer */}
        <div className="mt-auto flex flex-wrap items-center justify-between gap-2">
          {/* 信任徽标与连接徽标**同时在场**。
              此前这里是三元二选一（有连接就只显示连接），于是**连上的设备永远看不到
              自己的信任级别**——而那正是它最要紧的时刻：一台 `temporary` 或
              `blocked` 的设备连上来时，用户需要在决定发文件之前看见这件事。
              「待确认」（`trustConfirmed === false`）同理，此前也被一起吞掉。
              DESIGN.md 的 Device Card Contract 把这条记为 Known gap，Web 端两枚并排。

              连接徽标可点开链路详情，见 connection-badge.tsx。它的条件只看
              `connection`：延迟要等第一次 ping（30s 间隔），此前 `latency` 为 null，
              把它并进条件等于「刚连上的半分钟里连接方式不显示」。 */}
          <span className="flex min-w-0 flex-wrap items-center gap-1.5">
            <TrustBadge device={device} />
            {device.connection && <ConnectionBadge device={device} />}
          </span>

          {/* Action Button */}
          {device.isPaired ? (
            <Button
              size="sm"
              variant={canSend ? "default" : "outline"}
              disabled={!canSend}
              data-testid="device-send-action"
              onClick={(e) => {
                e.stopPropagation();
                onSend?.(device);
              }}
              className={cn(
                "h-auto shrink-0 gap-1.5 rounded-full px-3 py-1.5 text-xs transition-[background-color,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.97]",
                canSend
                  ? "shadow-[0_8px_18px_rgba(219,163,65,0.18)]"
                  : "glass-control border-transparent text-muted-foreground"
              )}
            >
              <Send className="size-3.5" />
              {/* 按钮**不重复禁用原因**。「灰按钮要解释自己」在这张卡上已经由别处满足：
                  离线由状态行的「● 离线」说，已阻止由上面恒在场的信任徽标说（它此前被
                  连接徽标顶掉，才让按钮不得不兼职解释）。在一张 132px 高的卡里把同一句话
                  说两遍，比不说更糟。
                  Web 的卡片是另一种版式（按钮独占底部一整行、状态在顶部），那边由按钮承担
                  这句话是合适的——契约允许版式分叉，不允许信息缺失。 */}
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
        deviceName={displayName}
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
  onOrganizeClick,
  onPolicyClick,
  onUnpairClick,
  label,
}: {
  onOrganizeClick?: () => void;
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
        {onOrganizeClick && (
          <DropdownMenuItem
            onSelect={(event) => {
              event.stopPropagation();
              onOrganizeClick();
            }}
            data-testid="device-organize-menu-action"
            className="h-8 rounded-[10px] px-2.5 text-xs font-medium"
          >
            <Tags className="size-3.5" />
            <Trans>设备别名与分组</Trans>
          </DropdownMenuItem>
        )}
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

  // 走共享包的归一化，不手抄 `?? "collaborator"`——那份默认值的语义（「内核允许为空，
  // UI 一律按协作者呈现」）住在 `shared-view`，三端各抄一份迟早会有人改错一处。
  const trust = trustConfig(normalizeTrustLevel(device.trustLevel));
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
