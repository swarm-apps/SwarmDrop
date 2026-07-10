/**
 * StartNodeSheet
 * 启动节点确认弹窗(桌面端 Dialog)
 */

import { useNetworkStore } from "@/stores/network-store";
import { useShallow } from "zustand/shallow";
import { Trans } from "@lingui/react/macro";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Card, CardContent } from "@/components/ui/card";

interface StartNodeSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function StartNodeSheet({ open, onOpenChange }: StartNodeSheetProps) {
  const { startNetwork, status } = useNetworkStore(
    useShallow((s) => ({
      startNetwork: s.startNetwork,
      status: s.status,
    })),
  );

  const isStarting = status === "starting";

  const handleStart = async () => {
    const ok = await startNetwork();
    // 失败时保持弹窗打开，让用户看到错误状态与 toast 提示
    if (ok) onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>网络节点</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>管理 P2P 网络节点的启动和连接状态</Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <span className="text-sm text-muted-foreground">
              <Trans>节点状态</Trans>
            </span>
            <Badge
              variant="outline"
              className="gap-1.5 border-transparent bg-muted text-muted-foreground"
            >
              <span className="size-2 rounded-full bg-muted-foreground" />
              <Trans>未启动</Trans>
            </Badge>
          </div>

          <Separator />

          <div className="flex flex-col gap-2">
            <span className="text-sm text-muted-foreground">
              <Trans>监听地址</Trans>
            </span>
            <Card className="gap-0 bg-muted/50 py-0">
              <CardContent className="p-3">
                <span className="text-sm text-muted-foreground">
                  <Trans>节点未启动</Trans>
                </span>
              </CardContent>
            </Card>
          </div>

          <Separator />

          <div className="grid grid-cols-2 gap-4">
            <Card className="gap-0 bg-muted/50 py-0">
              <CardContent className="flex flex-col gap-1 p-3">
                <span className="text-xs text-muted-foreground">
                  <Trans>已连接节点</Trans>
                </span>
                <span className="text-2xl font-semibold text-foreground">0</span>
              </CardContent>
            </Card>
            <Card className="gap-0 bg-muted/50 py-0">
              <CardContent className="flex flex-col gap-1 p-3">
                <span className="text-xs text-muted-foreground">
                  <Trans>已发现节点</Trans>
                </span>
                <span className="text-2xl font-semibold text-foreground">0</span>
              </CardContent>
            </Card>
          </div>
        </div>

        <DialogFooter>
          <Button
            onClick={handleStart}
            disabled={isStarting}
            data-testid="start-node-confirm-action"
          >
            {isStarting ? <Trans>启动中...</Trans> : <Trans>启动节点</Trans>}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
