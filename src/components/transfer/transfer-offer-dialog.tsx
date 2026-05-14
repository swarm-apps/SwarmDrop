import { useState, useEffect, useMemo, useCallback, memo } from "react";
import { Download, FolderOpen } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { Button } from "@/components/ui/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
  ResponsiveDialogDescription,
} from "@/components/responsive-dialog";
import { Trans } from "@lingui/react/macro";
import { useTransferStore } from "@/stores/transfer-store";
import {
  acceptReceive,
  rejectReceive,
  type SaveLocation,
} from "@/commands/transfer";
import { FileTree } from "@/routes/_app/send/-components/file-tree";
import { buildTreeDataFromOffer } from "@/routes/_app/send/-file-tree";
import { pickFolder, getDefaultSavePath } from "@/lib/file-picker";
import { useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import { getErrorMessage } from "@/lib/errors";

export function TransferOfferDialog() {
  const navigate = useNavigate();
  const [savePath, setSavePath] = useState("");
  const [processing, setProcessing] = useState(false);
  const [dismissedSessionId, setDismissedSessionId] = useState<string | null>(
    null,
  );

  const { shiftOffer, pendingOffers, addSession } = useTransferStore(
    useShallow((s) => ({
      shiftOffer: s.shiftOffer,
      pendingOffers: s.pendingOffers,
      addSession: s.addSession,
    })),
  );

  // 获取当前要显示的 offer（队列第一个且未被用户关闭的）
  const currentOffer = useMemo(() => {
    if (pendingOffers.length === 0) return null;
    const first = pendingOffers[0];
    if (first.sessionId === dismissedSessionId) return null;
    return first;
  }, [pendingOffers, dismissedSessionId]);

  useEffect(() => {
    let cancelled = false;
    getDefaultSavePath().then((path) => {
      if (!cancelled) setSavePath(path);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // 当 dismissedSessionId 对应的 offer 被移除后，清除 dismissedSessionId
  useEffect(() => {
    if (
      dismissedSessionId &&
      !pendingOffers.some((o) => o.sessionId === dismissedSessionId)
    ) {
      setDismissedSessionId(null);
    }
  }, [pendingOffers, dismissedSessionId]);

  const treeData = useMemo(() => {
    if (!currentOffer) return null;
    return buildTreeDataFromOffer(currentOffer.files);
  }, [currentOffer]);

  const handleChangePath = useCallback(async () => {
    const selected = await pickFolder();
    if (selected) {
      setSavePath(selected);
    }
  }, []);

  const handleAccept = useCallback(async () => {
    if (!currentOffer) return;
    setProcessing(true);
    try {
      const saveLocation: SaveLocation = { type: "path", path: savePath };

      await acceptReceive(currentOffer.sessionId, saveLocation);

      addSession({
        sessionId: currentOffer.sessionId,
        direction: "receive",
        peerId: currentOffer.peerId,
        deviceName: currentOffer.deviceName,
        files: currentOffer.files,
        totalSize: currentOffer.totalSize,
        status: "transferring",
        progress: null,
        error: null,
        startedAt: Date.now(),
        completedAt: null,
        saveLocation,
      });

      // 从队列移除并跳转到详情页
      navigate({
        to: "/transfer/$sessionId",
        params: { sessionId: currentOffer.sessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
      shiftOffer();
    }
  }, [currentOffer, savePath, addSession, navigate, shiftOffer]);

  const handleReject = useCallback(async () => {
    if (!currentOffer) return;
    setProcessing(true);
    try {
      await rejectReceive(currentOffer.sessionId);
      // 从队列移除
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
      shiftOffer();
    }
  }, [currentOffer, shiftOffer]);

  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open && !processing) {
        handleReject();
      }
    },
    [processing, handleReject],
  );

  if (!currentOffer || !treeData) return null;

  return (
    <ResponsiveDialog open={true} onOpenChange={handleOpenChange} forceDialog>
      <ResponsiveDialogContent
        className="sm:max-w-lg"
        showCloseButton={false}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <ResponsiveDialogHeader className="flex flex-col items-center gap-2">
          <div className="flex size-14 items-center justify-center rounded-full bg-blue-100">
            <Download className="size-7 text-blue-600" />
          </div>
          <ResponsiveDialogTitle className="text-center text-xl">
            <Trans>收到文件</Trans>
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription className="text-center">
            <Trans>来自 {currentOffer.deviceName}</Trans>
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>

        <div className="flex-1 overflow-y-auto px-4 sm:px-0">
          <div className="max-h-[40vh] min-h-30">
            <FileTree
              mode="select"
              dataLoader={treeData.dataLoader}
              rootChildren={treeData.rootChildren}
              totalCount={currentOffer.files.length}
              totalSize={currentOffer.totalSize}
              showHeader={false}
            />
          </div>

          <div className="mt-4">
            <SavePathSelector
              savePath={savePath}
              onChangePath={handleChangePath}
              disabled={processing}
            />
          </div>
        </div>

        <ResponsiveDialogFooter className="flex-row justify-center gap-3 sm:justify-center">
          <Button
            variant="outline"
            onClick={handleReject}
            disabled={processing}
            className="flex-1"
          >
            <Trans>拒绝</Trans>
          </Button>
          <Button
            onClick={handleAccept}
            disabled={processing}
            className="flex-1"
          >
            {processing ? <Trans>处理中...</Trans> : <Trans>接收</Trans>}
          </Button>
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

const SavePathSelector = memo(function SavePathSelector({
  savePath,
  onChangePath,
  disabled,
}: {
  savePath: string;
  onChangePath: () => void;
  disabled: boolean;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-sm font-medium text-foreground">
        <Trans>保存到</Trans>
      </span>
      <div className="flex items-center gap-2 rounded-lg border border-border px-3 py-2.5">
        <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm text-foreground">
          {savePath}
        </span>
        <Button
          size="sm"
          variant="ghost"
          className="h-6 shrink-0 px-2 text-xs"
          onClick={onChangePath}
          disabled={disabled}
        >
          <Trans>更改</Trans>
        </Button>
      </div>
    </div>
  );
});
