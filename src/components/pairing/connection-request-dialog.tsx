/**
 * ConnectionRequestDialog
 * 收到配对请求弹窗：显示对方设备信息 + 安全提示 + 接受/拒绝
 */

import { ShieldCheck, Monitor, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { useNavigate } from "@tanstack/react-router";
import { usePairingStore } from "@/stores/pairing-store";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { deviceDisplayName } from "@/lib/device-name";

export function ConnectionRequestDialog() {
  const navigate = useNavigate();
  const { incomingRequest, acceptRequest, rejectRequest } = usePairingStore(
    useShallow((state) => ({
      incomingRequest: state.incomingRequest,
      acceptRequest: state.acceptRequest,
      rejectRequest: state.rejectRequest,
    }))
  );

  const isOpen = incomingRequest !== null;

  const DeviceIcon = incomingRequest ? getDeviceIcon(incomingRequest.osInfo.os) : Monitor;

  const handleAccept = async () => {
    const success = await acceptRequest();
    if (success) {
      navigate({ to: "/devices" });
    }
  };

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      rejectRequest();
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader className="flex flex-col items-center gap-2">
          <div className="flex size-12 items-center justify-center rounded-full bg-primary/10">
            <ShieldCheck className="size-6 text-primary" />
          </div>
          <DialogTitle className="text-center text-xl">
            <Trans>连接请求</Trans>
          </DialogTitle>
          <DialogDescription className="text-center">
            <Trans>一台设备正在请求与您配对</Trans>
          </DialogDescription>
        </DialogHeader>

        {incomingRequest && (
          <div className="flex flex-col items-center gap-4 py-2">
            {/* 设备信息 */}
            <div className="flex items-center gap-3 rounded-lg border border-border bg-card p-4">
              <div className="flex size-10 items-center justify-center rounded-full bg-muted">
                <DeviceIcon className="size-5 text-primary" />
              </div>
              <div className="flex flex-col">
                <span className="text-sm font-medium text-foreground">
                  {deviceDisplayName(incomingRequest.osInfo)}
                </span>
                <span className="text-xs text-muted-foreground">
                  {incomingRequest.osInfo.platform} · {incomingRequest.osInfo.os}
                </span>
              </div>
            </div>

            {/* 安全提示 */}
            <div className="flex items-start gap-2 rounded-lg bg-amber-50 p-3 text-amber-800 dark:bg-amber-950 dark:text-amber-200">
              <AlertTriangle className="mt-0.5 size-4 shrink-0" />
              <p className="text-xs">
                <Trans>
                  配对后，该设备可以向您发送文件。请确认您认识此设备。
                </Trans>
              </p>
            </div>
          </div>
        )}

        <DialogFooter className="flex-row justify-center gap-3 sm:justify-center">
          <Button variant="outline" onClick={() => rejectRequest()}>
            <Trans>拒绝</Trans>
          </Button>
          <Button onClick={handleAccept}>
            <Trans>接受配对</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
