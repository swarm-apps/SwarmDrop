/**
 * AddDeviceMenu
 * "连接设备"下拉菜单：生成配对码 / 输入配对码
 */

import { Link, Keyboard, Plus } from "lucide-react";
import { useNavigate } from "@tanstack/react-router";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Trans } from "@lingui/react/macro";

interface AddDeviceMenuProps {
  /** trigger 样式变体:default = 顶栏主按钮,compact = 紧凑按钮 */
  variant?: "default" | "compact";
}

export function AddDeviceMenu({ variant = "compact" }: AddDeviceMenuProps = {}) {
  const navigate = useNavigate();

  const triggerClass =
    variant === "default"
      ? "h-9 gap-1.5 rounded-lg px-3.5 text-[13px] font-medium"
      : "h-auto gap-1.5 rounded-md px-3 py-1.5 text-[13px] font-medium";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" className={triggerClass}>
          <Plus className="size-4" />
          <Trans>添加设备</Trans>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuItem onClick={() => navigate({ to: "/pairing/generate" })}>
          <Link className="size-4" />
          <Trans>生成配对码</Trans>
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => navigate({ to: "/pairing/input" })}>
          <Keyboard className="size-4" />
          <Trans>输入配对码</Trans>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
