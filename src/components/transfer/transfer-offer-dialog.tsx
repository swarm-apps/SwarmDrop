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
import { FileBrowser, fromOfferFiles } from "@/components/file-browser";
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
  const configuredSavePath = usePreferencesStore((s) => s.transfer.savePath);
  const fileView = usePreferencesStore((s) => s.fileBrowserViews.transfer);
  const setFileBrowserView = usePreferencesStore((s) => s.setFileBrowserView);

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
    if (configuredSavePath) {
      setSavePath(configuredSavePath);
    } else {
      getDefaultSavePath().then((path) => {
        if (!cancelled) setSavePath(path);
      });
    }
    return () => {
      cancelled = true;
    };
  }, [configuredSavePath]);

  // 当 dismissedSessionId 对应的 offer 被移除后，清除 dismissedSessionId
  useEffect(() => {
    if (
      dismissedSessionId &&
      !pendingOffers.some((o) => o.sessionId === dismissedSessionId)
    ) {
      setDismissedSessionId(null);
    }
  }, [pendingOffers, dismissedSessionId]);

  const offerItems = useMemo(() => {
    if (!currentOffer) return [];
    return fromOfferFiles(currentOffer.files);
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

      // 成功后才出队 + 跳转活动中心并选中该会话；失败时保留 offer 供重试（不在 finally 出队）。
      shiftOffer();
      navigate({
        to: "/transfer",
        search: { session: currentOffer.sessionId },
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

  if (!currentOffer) return null;

  return (
    <Dialog open={true} onOpenChange={handleOpenChange}>
      <DialogContent
        data-testid="transfer-offer-dialog"
        className="flex max-h-[min(820px,calc(100dvh-2rem))] flex-col gap-0 overflow-hidden rounded-[20px] p-0 sm:max-w-2xl"
        showCloseButton={false}
        onPointerDownOutside={(e) => e.preventDefault()}
      >
        <DialogHeader className="shrink-0 border-b border-border/60 px-5 py-4 text-left sm:text-left">
          <div className="flex min-w-0 items-start gap-3.5">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-[15px] bg-primary/12 text-brand">
              <Download className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <DialogTitle className="text-lg leading-6">
                <Trans>收到文件</Trans>
              </DialogTitle>
              <DialogDescription className="mt-0.5 truncate">
                <Trans>来自 {currentOffer.deviceName}</Trans>
              </DialogDescription>
              <div className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5">
                {currentOffer.origin.type === "mcp" && (
                  <Badge variant="secondary" className="max-w-full gap-1">
                    <Bot className="size-3.5 shrink-0" />
                    <span className="truncate">
                      {currentOffer.origin.client ? (
                        <Trans>由 AI 代理发起（{currentOffer.origin.client}）</Trans>
                      ) : (
                        <Trans>由 AI 代理发起</Trans>
                      )}
                    </span>
                  </Badge>
                )}
                <PolicyReasonBadge
                  variant="offer"
                  policyAction={currentOffer.policyAction}
                  policyReason={currentOffer.policyReason}
                />
              </div>
            </div>
          </div>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          <FileBrowser
            items={offerItems}
            title={<Trans>文件</Trans>}
            view={fileView}
            onViewChange={(nextView) =>
              setFileBrowserView("transfer", nextView)
            }
            className="h-[clamp(280px,42vh,420px)] min-h-0 flex-none"
          />

          <SavePathSelector
            savePath={savePath}
            onChangePath={handleChangePath}
            disabled={processing}
          />
        </div>

        <DialogFooter className="shrink-0 flex-row gap-3 border-t border-border/60 bg-muted/20 px-5 py-4 sm:justify-end">
          <Button
            variant="outline"
            onClick={handleReject}
            disabled={processing}
            className="h-10 flex-1 rounded-xl sm:flex-none sm:px-7"
          >
            <Trans>拒绝</Trans>
          </Button>
          <Button
            onClick={handleAccept}
            disabled={processing || !savePath}
            className="h-10 flex-1 rounded-xl sm:flex-none sm:px-8"
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
      <div className="flex items-center gap-2 rounded-xl border border-border/70 bg-muted/20 px-3 py-2.5">
        <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm text-foreground">
          {savePath}
        </span>
        <Button
          size="sm"
          variant="ghost"
          className="h-7 shrink-0 rounded-lg px-2.5 text-xs"
          onClick={onChangePath}
          disabled={disabled}
        >
          <Trans>更改</Trans>
        </Button>
      </div>
    </div>
  );
});
