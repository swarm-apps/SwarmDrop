"use client";

// 已配对设备卡片。**逐条对照 `DESIGN.md` 的 Device Card Contract 实现**——那份契约同时约束
// 桌面 / 移动 / Web 三份实现，改这个文件前先读它。
//
// 与桌面卡片的形态差异是契约允许的：桌面是玻璃拟态 + hover 态 + 固定 132px 卡高，那些假设在
// 375px 触摸设备上是负资产（hover 不存在、固定高在两列网格里过高、backdrop-filter 在移动 GPU
// 上有实打实的开销）。这里是扁平卡 + 响应式高度，移动优先。
//
// 契约里**不允许**分叉的是信息位：8 项一个都不能因为「布局太挤」省掉。桌面端目前把信任徽标
// 与连接徽标写成三元、二选一，那是已记录的既有缺口（见 DESIGN.md 的 Known gap），不是可抄的先例。

import { Trans, useLingui } from "@lingui/react/macro";
import { MoreHorizontal, Send, ShieldCheck, Tags, Unlink } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import {
  canSendToDevice,
  deviceIdentityHint,
  normalizeTrustLevel,
  organizedDeviceName,
  type DeviceOrganization,
} from "@swarmdrop/shared-view";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/cn";
import { TRUST_META, deviceIcon } from "../_lib/device-presentation";
import { sendToPeerHref } from "../_lib/nav";
import type { Device } from "../_lib/view-types";
import { ConnectionBadge } from "./connection-badge";
import { StatusDot } from "./status-dot";
import { UnpairDialog } from "./unpair-dialog";

interface DeviceCardProps {
  device: Device;
  organization: DeviceOrganization;
  /** 该设备所属分组名（已排序），由调用方从组织派生——卡片不自己算，避免每张卡重复遍历。 */
  groupNames: string[];
  /** 当前渲染的这批设备里存在同名设备时为 true，触发次级身份行。 */
  showIdentityHint: boolean;
  onOrganize: (device: Device) => void;
  onEditPolicy: (device: Device) => void;
}

export function DeviceCard({
  device,
  organization,
  groupNames,
  showIdentityHint,
  onOrganize,
  onEditPolicy,
}: DeviceCardProps) {
  const { t } = useLingui();
  const [unpairOpen, setUnpairOpen] = useState(false);

  const displayName = organizedDeviceName(device, organization);
  const Icon = deviceIcon(device.os);
  const isOnline = device.status === "online";
  const sendable = canSendToDevice(device);
  const trust = normalizeTrustLevel(device.trustLevel);
  // 契约第 4 项：只在同名或有分组时出现，否则这一行是纯噪声。
  const identityLine =
    showIdentityHint || groupNames.length > 0
      ? [...groupNames, showIdentityHint ? deviceIdentityHint(device) : null]
          .filter(Boolean)
          .join(" · ")
      : null;

  return (
    <>
      <div
        data-testid="device-card"
        data-peer-id={device.peerId}
        className={cn(
          "flex w-full flex-col gap-3 rounded-xl border bg-card p-4 shadow-xs transition-colors",
          // 契约：主动作不可用时整卡视觉降级。不做 hover 态——触摸设备上没有 hover，
          // 靠它表达可交互性等于对一半用户不表达。
          !isOnline && "opacity-65",
        )}
      >
        <div className="flex items-start gap-3">
          <div
            className={cn(
              "flex size-10 shrink-0 items-center justify-center rounded-lg",
              isOnline ? "bg-[var(--brand-solid)]/12 text-brand" : "bg-muted text-muted-foreground",
            )}
          >
            <Icon className="size-5" aria-hidden />
          </div>

          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <p className="truncate text-sm font-medium text-foreground">{displayName}</p>
            <span className="flex items-center gap-1.5 text-xs">
              {/* 契约第 3 项：状态点**与**文案同时在场。只有色点不满足——色盲用户读不到它。 */}
              <StatusDot colorClass={isOnline ? "bg-emerald-500" : "bg-muted-foreground"} />
              <span className={isOnline ? "text-success-ink" : "text-muted-foreground"}>
                {isOnline ? <Trans>在线</Trans> : <Trans>离线</Trans>}
              </span>
            </span>
            {identityLine && (
              <p className="truncate text-[11px] text-muted-foreground">{identityLine}</p>
            )}
          </div>

          <DeviceActionMenu
            label={t`设备操作`}
            onOrganize={() => onOrganize(device)}
            onEditPolicy={() => onEditPolicy(device)}
            onUnpair={() => setUnpairOpen(true)}
          />
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          <Badge variant="secondary" className={cn("gap-1 border-transparent", TRUST_META[trust].className)}>
            <TrustLabel trust={trust} />
            {device.trustConfirmed === false && (
              <span className="opacity-70">
                · <Trans>待确认</Trans>
              </span>
            )}
          </Badge>

          {/* 契约第 6 项：在线且已知连接方式时，连接徽标与延迟一起出（徽标可点开链路详情）。
              离线时它不渲染，但卡片其余部分不动——高度靠上面的 flex 结构撑住，不会抖。 */}
          <ConnectionBadge device={device} />
        </div>

        {/* 契约的 Send Entry：发送从设备卡片进入，目标已预选。
            离线设备**不给可点的入口**——点进去也只会收到一条内核报错。 */}
        <div className="mt-auto">
          {sendable ? (
            <Button asChild size="sm" className="w-full gap-1.5">
              <Link href={sendToPeerHref(device.peerId)} data-testid="device-send-action">
                <Send className="size-4" aria-hidden />
                <Trans>发送</Trans>
              </Link>
            </Button>
          ) : (
            <Button size="sm" variant="secondary" disabled className="w-full gap-1.5">
              <Send className="size-4" aria-hidden />
              {trust === "blocked" ? <Trans>已阻止</Trans> : <Trans>离线</Trans>}
            </Button>
          )}
        </div>
      </div>

      <UnpairDialog
        open={unpairOpen}
        onOpenChange={setUnpairOpen}
        peerId={device.peerId}
        deviceName={displayName}
      />
    </>
  );
}

/**
 * 契约第 8 项：取消配对是最低要求，信任策略与别名/分组是「where supported」——
 * Web 两者都支持，所以三项齐全（策略走 `WebNode.update_paired_device_policy`，
 * 别名/分组是纯本机偏好）。
 */
function DeviceActionMenu({
  label,
  onOrganize,
  onEditPolicy,
  onUnpair,
}: {
  label: string;
  onOrganize: () => void;
  onEditPolicy: () => void;
  onUnpair: () => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={label}
          title={label}
          data-testid="device-actions-menu"
          // 触摸目标 ≥44×44：视觉上是 32px 的圆，点击区靠负 margin 撑到 44 而不占版面。
          className="-m-1.5 flex size-11 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          <MoreHorizontal className="size-4" aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-40">
        <DropdownMenuItem onSelect={onOrganize} data-testid="device-organize-action">
          <Tags className="size-4" aria-hidden />
          <Trans>别名与分组</Trans>
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onEditPolicy} data-testid="device-policy-action">
          <ShieldCheck className="size-4" aria-hidden />
          <Trans>信任策略</Trans>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onSelect={onUnpair} data-testid="device-unpair-action">
          <Unlink className="size-4" aria-hidden />
          <Trans>取消配对</Trans>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * 徽标文案按枚举值 switch 出 `<Trans>`，而不是从 `_lib/device-presentation.ts` 的映射里取
 * 字符串——翻译宏只在组件里展开，放进纯数据模块就成了永远的中文。
 */
function TrustLabel({ trust }: { trust: ReturnType<typeof normalizeTrustLevel> }) {
  switch (trust) {
    case "owned":
      return <Trans>本人设备</Trans>;
    case "temporary":
      return <Trans>临时设备</Trans>;
    case "blocked":
      return <Trans>已阻止</Trans>;
    default:
      return <Trans>协作者</Trans>;
  }
}
