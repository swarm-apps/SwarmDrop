/**
 * Device Found Views
 * 设备详情页面：显示通过配对码找到的设备信息
 * 移动端（MobileDeviceFoundView）和桌面端（DesktopDeviceFoundContent）
 */

import { useRef } from "react";
import {
  ArrowLeft,
  Hash,
  Link as LinkIcon,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { Trans } from "@lingui/react/macro";
import type { DeviceInfo } from "@/commands/pairing";
import { usePairingStore } from "@/stores/pairing-store";
import { deviceDisplayName } from "@/lib/device-name";

function formatPlatformDisplay(platform: string): string {
  const map: Record<string, string> = {
    macos: "macOS",
    windows: "Windows",
    linux: "Linux",
    ios: "iOS",
    android: "Android",
  };
  return map[platform.toLowerCase()] ?? platform;
}

/**
 * 从 pairing store 提取 device found 状态
 * 使用 ref 记住 deviceInfo，避免 requesting 阶段丢失数据
 */
export function useDeviceFoundState() {
  const current = usePairingStore((s) => s.current);
  const deviceInfoRef = useRef<DeviceInfo | null>(null);

  if (current.phase === "found") {
    deviceInfoRef.current = current.deviceInfo;
  } else if (
    current.phase === "idle" ||
    current.phase === "inputting" ||
    current.phase === "generating"
  ) {
    deviceInfoRef.current = null;
  }

  const isActive = current.phase === "found" || current.phase === "requesting";
  const deviceInfo = deviceInfoRef.current;

  return {
    showDeviceFound: isActive && deviceInfo !== null,
    deviceInfo,
    isRequesting: current.phase === "requesting",
  };
}

/* ─────────────────── 共享子组件 ─────────────────── */

/** 设备头部：图标 + 名称 + 系统描述 */
function DeviceHeader({
  hostname,
  platform,
  arch,
  size = "lg",
}: {
  hostname: string;
  platform: string;
  arch: string;
  size?: "lg" | "md";
}) {
  const DeviceIcon = getDeviceIcon(platform);
  const osDisplay = formatPlatformDisplay(platform);

  return (
    <div className="flex flex-col items-center gap-3">
      <div
        className={
          size === "lg"
            ? "flex size-[72px] items-center justify-center rounded-[20px] bg-blue-600/8"
            : "flex size-16 items-center justify-center rounded-full bg-blue-50"
        }
      >
        <DeviceIcon
          className={size === "lg" ? "size-8 text-blue-600" : "size-7 text-blue-600"}
        />
      </div>
      <div className="flex flex-col items-center gap-1">
        <span
          className={
            size === "lg"
              ? "text-lg font-semibold text-foreground"
              : "text-xl font-semibold text-foreground"
          }
        >
          {hostname}
        </span>
        <span className="text-sm text-muted-foreground">
          {osDisplay} · {arch}
        </span>
      </div>
    </div>
  );
}

/** 设备信息列表：配对方式 / 操作系统 / CPU 架构 */
function InfoList({ platform, arch }: { platform: string; arch: string }) {
  const osDisplay = formatPlatformDisplay(platform);

  return (
    <div className="overflow-hidden rounded-xl border border-border text-[13px]">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-muted-foreground">
          <Trans>配对方式</Trans>
        </span>
        <div className="flex items-center gap-1.5">
          <Hash className="size-3.5 text-blue-600" />
          <span className="font-medium text-foreground">
            <Trans>配对码</Trans>
          </span>
        </div>
      </div>
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-muted-foreground">
          <Trans>操作系统</Trans>
        </span>
        <span className="font-medium text-foreground">{osDisplay}</span>
      </div>
      <div className="flex items-center justify-between px-4 py-3">
        <span className="text-muted-foreground">
          <Trans>CPU 架构</Trans>
        </span>
        <span className="font-medium text-foreground">{arch}</span>
      </div>
    </div>
  );
}

/** 在线状态条 */
function OnlineStatus() {
  return (
    <div className="flex items-center justify-center gap-2 rounded-lg bg-green-50 px-4 py-2 dark:bg-green-500/10">
      <span className="size-2 rounded-full bg-green-600 dark:bg-green-400" />
      <span className="text-[13px] font-medium text-green-800 dark:text-green-400">
        <Trans>设备在线，可以建立连接</Trans>
      </span>
    </div>
  );
}

/* ─────────────────── 移动端视图 ─────────────────── */

export function MobileDeviceFoundView({
  deviceInfo,
  isRequesting,
  onSendRequest,
  onBack,
}: {
  deviceInfo: DeviceInfo;
  isRequesting: boolean;
  onSendRequest: () => void;
  onBack: () => void;
}) {
  const { codeRecord } = deviceInfo;

  return (
    <>
      {/* Header */}
      <header className="flex items-center px-5 py-3">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-2 text-foreground"
        >
          <ArrowLeft className="size-5" />
          <span className="text-lg font-semibold">
            <Trans>设备详情</Trans>
          </span>
        </button>
      </header>

      {/* Content */}
      <div className="flex flex-1 flex-col gap-6 overflow-auto px-5 pt-10">
        {/* Device Card */}
        <div className="flex flex-col items-center gap-5 rounded-2xl border border-border p-8">
          <DeviceHeader
            hostname={deviceDisplayName(codeRecord)}
            platform={codeRecord.platform}
            arch={codeRecord.arch}
            size="lg"
          />
        </div>

        {/* Info List */}
        <InfoList platform={codeRecord.platform} arch={codeRecord.arch} />

        {/* Status */}
        <OnlineStatus />

        {/* Footer */}
        <div className="flex flex-col gap-3 pt-4">
          <Button
            onClick={onSendRequest}
            disabled={isRequesting}
            className="h-11 w-full bg-blue-600 hover:bg-blue-700"
            size="lg"
          >
            {isRequesting ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <LinkIcon className="size-4" />
            )}
            {isRequesting ? <Trans>等待对方确认...</Trans> : <Trans>发起配对</Trans>}
          </Button>
          <Button
            variant="outline"
            onClick={onBack}
            disabled={isRequesting}
            className="h-11 w-full"
            size="lg"
          >
            <Trans>取消</Trans>
          </Button>
        </div>
      </div>
    </>
  );
}

/* ─────────────────── 桌面端内容 ─────────────────── */

export function DesktopDeviceFoundContent({
  deviceInfo,
  isRequesting,
  onSendRequest,
  onCancel,
}: {
  deviceInfo: DeviceInfo;
  isRequesting: boolean;
  onSendRequest: () => void;
  onCancel: () => void;
}) {
  const { codeRecord } = deviceInfo;

  return (
    <div className="flex w-95 flex-col gap-5">
      {/* Device Header */}
      <DeviceHeader
        hostname={deviceDisplayName(codeRecord)}
        platform={codeRecord.platform}
        arch={codeRecord.arch}
        size="md"
      />

      {/* Info List */}
      <InfoList platform={codeRecord.platform} arch={codeRecord.arch} />

      {/* Status */}
      <OnlineStatus />

      {/* Actions */}
      <div className="flex items-center justify-center gap-3 pt-1">
        <Button variant="outline" onClick={onCancel} disabled={isRequesting}>
          <Trans>取消</Trans>
        </Button>
        <Button
          onClick={onSendRequest}
          disabled={isRequesting}
          className="bg-blue-600 hover:bg-blue-700"
        >
          {isRequesting && <Loader2 className="size-4 animate-spin" />}
          {isRequesting ? <Trans>等待对方确认...</Trans> : <Trans>发送配对请求</Trans>}
        </Button>
      </div>
    </div>
  );
}
