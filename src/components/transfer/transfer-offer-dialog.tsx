import { useState, useEffect, useMemo, useCallback, memo } from "react";
import { Download, FolderOpen, Bot } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
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
import { useTransferStore } from "@/stores/transfer-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { commands } from "@/lib/bindings";
import type { SaveLocation } from "@/lib/types";
import { FileTree } from "@/components/file-tree";
import { buildTreeDataFromOffer } from "@/components/file-tree";
import { PolicyReasonBadge } from "@/components/transfer/policy-reason-badge";
import { Badge } from "@/components/ui/badge";
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

  const { shiftOffer, pendingOffers, loadProjections } = useTransferStore(
    useShallow((s) => ({
      shiftOffer: s.shiftOffer,
      pendingOffers: s.pendingOffers,
      loadProjections: s.loadProjections,
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
    // 默认落盘位置 = 设置里配的接收文件夹（preferences.transfer.savePath）；未配则回退
    // getDefaultSavePath（<下载>/SwarmDrop）。agent 代收也读同一个 pref，二者一致。
    const configured = usePreferencesStore.getState().transfer.savePath;
    if (configured) {
      setSavePath(configured);
    } else {
      getDefaultSavePath().then((path) => {
        if (!cancelled) setSavePath(path);
      });
    }
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

      await commands.acceptReceive(currentOffer.sessionId, saveLocation);
      await loadProjections();

      // 成功后才出队 + 跳转详情；失败时保留 offer 供重试（不在 finally 出队）。
      shiftOffer();
      navigate({
        to: "/transfer/$sessionId",
        params: { sessionId: currentOffer.sessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
    }
  }, [currentOffer, savePath, loadProjections, navigate, shiftOffer]);

  const handleReject = useCallback(async () => {
    if (!currentOffer) return;
    setProcessing(true);
    try {
      await commands.rejectReceive(currentOffer.sessionId);
      await loadProjections();
      // 成功后才出队；失败时保留 offer 供重试（不在 finally 出队）。
      shiftOffer();
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
    }
  }, [currentOffer, loadProjections, shiftOffer]);

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
    <Dialog open={true} onOpenChange={handleOpenChange}>
      <DialogContent
        className="sm:max-w-lg"
        showCloseButton={false}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <DialogHeader className="flex flex-col items-center gap-2">
          <div className="flex size-14 items-center justify-center rounded-full bg-primary/15">
            <Download className="size-7 text-brand" />
          </div>
          <DialogTitle className="text-center text-xl">
            <Trans>收到文件</Trans>
          </DialogTitle>
          <DialogDescription className="text-center">
            <Trans>来自 {currentOffer.deviceName}</Trans>
          </DialogDescription>
          {currentOffer.origin.type === "mcp" && (
            <Badge variant="secondary" className="gap-1">
              <Bot className="size-3.5" />
              {currentOffer.origin.client ? (
                <Trans>由 AI 代理发起（{currentOffer.origin.client}）</Trans>
              ) : (
                <Trans>由 AI 代理发起</Trans>
              )}
            </Badge>
          )}
          <PolicyReasonBadge
            variant="offer"
            policyAction={currentOffer.policyAction}
            policyReason={currentOffer.policyReason}
          />
        </DialogHeader>

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

        <DialogFooter className="flex-row justify-center gap-3 sm:justify-center">
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
        </DialogFooter>
      </DialogContent>
    </Dialog>
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
