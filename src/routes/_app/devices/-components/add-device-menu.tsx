/**
 * AddDeviceMenu
 *
 * 「添加设备」入口（保留旧文件名兼容引用）—— 改 Popover：
 *   1. 附近在线设备列表（节点 running 时显示未配对 + online 的 peer）
 *   2. 生成配对码 / 输入配对码 两个动作
 *
 * 点附近设备触发 directPairing；点 menu 项跳 pairing 路由。
 */

import { Keyboard, Link, Plus, Radio } from "lucide-react";
import { useNavigate } from "@tanstack/react-router";
import { Trans } from "@lingui/react/macro";
import { useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useNetworkStore } from "@/stores/network-store";
import { usePairingStore } from "@/stores/pairing-store";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { deviceDisplayName } from "@/lib/device-name";
import { cn } from "@/lib/utils";

interface AddDeviceMenuProps {
  /** trigger 样式变体:default = 顶栏主按钮,compact = 紧凑按钮 */
  variant?: "default" | "compact";
}

export function AddDeviceMenu({
  variant = "compact",
}: AddDeviceMenuProps = {}) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);

  const isOnline = useNetworkStore(
    (s) => s.status === "running" || s.status === "starting",
  );
  const nearbyDevices = useNetworkStore(
    useShallow((s) =>
      s.devices.filter((d) => !d.isPaired && d.status === "online"),
    ),
  );
  const directPairing = usePairingStore((s) => s.directPairing);

  const triggerClass =
    variant === "default"
      ? "h-9 gap-1.5 rounded-lg px-3.5 text-[13px] font-medium"
      : "h-auto gap-1.5 rounded-md px-3 py-1.5 text-[13px] font-medium";

  const onPairNearby = (peerId: string) => {
    setOpen(false);
    directPairing(peerId);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" className={triggerClass}>
          <Plus className="size-4" />
          <Trans>添加设备</Trans>
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 p-0">
        {/* 附近在线设备 —— 节点运行时显示 */}
        {isOnline ? (
          <div className="border-b border-border">
            <div className="flex items-center justify-between px-3 pt-3 pb-2">
              <div className="flex items-center gap-1.5">
                <Radio className="size-3.5 text-muted-foreground" />
                <span className="text-[12px] font-semibold text-foreground">
                  <Trans>附近设备</Trans>
                </span>
                {nearbyDevices.length > 0 && (
                  <span className="rounded-full bg-secondary px-1.5 py-px text-[10px] font-medium text-muted-foreground">
                    {nearbyDevices.length}
                  </span>
                )}
              </div>
            </div>
            {nearbyDevices.length === 0 ? (
              <div className="px-3 pb-3 text-[11px] text-muted-foreground">
                <Trans>暂未发现附近设备 · 确保对端 SwarmDrop 已启动</Trans>
              </div>
            ) : (
              <ul className="max-h-60 overflow-auto px-1 pb-1">
                {nearbyDevices.map((d) => {
                  const Icon = getDeviceIcon(d.platform);
                  return (
                    <li key={d.peerId}>
                      <button
                        type="button"
                        onClick={() => onPairNearby(d.peerId)}
                        className={cn(
                          "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left",
                          "hover:bg-muted",
                        )}
                      >
                        <Icon className="size-4 text-blue-600 shrink-0" />
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-[13px] font-medium text-foreground">
                            {deviceDisplayName(d)}
                          </div>
                          <div className="truncate text-[10.5px] text-muted-foreground">
                            {d.platform} · {d.os}
                          </div>
                        </div>
                        <span className="rounded-md bg-primary/10 px-2 py-0.5 text-[10.5px] font-medium text-primary shrink-0">
                          <Trans>配对</Trans>
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        ) : null}

        {/* 配对码入口 —— 始终可见 */}
        <ul className="p-1">
          <li>
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                navigate({ to: "/pairing/generate" });
              }}
              className="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 hover:bg-muted"
            >
              <Link className="size-4 text-muted-foreground" />
              <span className="text-[13px] text-foreground">
                <Trans>生成配对码</Trans>
              </span>
            </button>
          </li>
          <li>
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                navigate({ to: "/pairing/input" });
              }}
              className="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 hover:bg-muted"
            >
              <Keyboard className="size-4 text-muted-foreground" />
              <span className="text-[13px] text-foreground">
                <Trans>输入配对码</Trans>
              </span>
            </button>
          </li>
        </ul>
      </PopoverContent>
    </Popover>
  );
}
