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
  const configuredSavePath = usePreferencesStore((s) => s.transfer.savePath);
  const fileView = usePreferencesStore((s) => s.fileBrowserViews.transfer);
  const setFileBrowserView = usePreferencesStore((s) => s.setFileBrowserView);

  const {
    pendingOffers,
    dismissedOfferIds,
    dismissOffer,
    removeOffer,
    loadProjections,
  } = useTransferStore(
    useShallow((s) => ({
      pendingOffers: s.pendingOffers,
      dismissedOfferIds: s.dismissedOfferIds,
      dismissOffer: s.dismissOffer,
      removeOffer: s.removeOffer,
      loadProjections: s.loadProjections,
    })),
  );

  // 队列里**第一条还没被关掉的**。此前是「队首，且队首没被关掉」——关掉一条就把后面
  // 所有 offer 一起堵死，第二条要等第一条被处理才见得到。
  const currentOffer = useMemo(
    () =>
      pendingOffers.find(
        (offer) => !dismissedOfferIds.includes(offer.sessionId),
      ) ?? null,
    [pendingOffers, dismissedOfferIds],
  );

  // 契约要求「等着的还有几条」要说出来（DESIGN.md Incoming Request Contract）。
  // 只数还没被关掉的：已关掉的那些在收件箱里找得到，不该在这里制造压迫感。
  const waitingCount = useMemo(
    () =>
      pendingOffers.reduce(
        (n, offer) =>
          dismissedOfferIds.includes(offer.sessionId) ||
          offer.sessionId === currentOffer?.sessionId
            ? n
            : n + 1,
        0,
      ),
    [pendingOffers, dismissedOfferIds, currentOffer],
  );

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
      removeOffer(currentOffer.sessionId);
      navigate({
        to: "/transfer",
        search: { session: currentOffer.sessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
    }
  }, [currentOffer, savePath, loadProjections, navigate, removeOffer]);

  const handleReject = useCallback(async () => {
    if (!currentOffer) return;
    setProcessing(true);
    try {
      await commands.rejectReceive(currentOffer.sessionId);
      await loadProjections();
      // 成功后才出队；失败时保留 offer 供重试（不在 finally 出队）。
      removeOffer(currentOffer.sessionId);
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setProcessing(false);
    }
  }, [currentOffer, loadProjections, removeOffer]);

  /**
   * 关闭弹窗 **≠ 拒绝**（DESIGN.md 的 Incoming Request Contract）。
   *
   * 此前这里直接调 `handleReject()`，而弹窗同时关掉了关闭按钮与「点外部关闭」，
   * 唯一出口是 Esc——按下去作废对方整次传输。对方只是在排队等，不是被阻塞着等答复，
   * 所以误点一下的代价必须远小于一次传输。
   *
   * 关闭后它仍在 `pendingOffers` 里，从收件箱的「待处理请求」区可以重新唤出。
   */
  const handleOpenChange = useCallback(
    (open: boolean) => {
      if (!open && !processing && currentOffer) {
        dismissOffer(currentOffer.sessionId);
      }
    },
    [processing, currentOffer, dismissOffer],
  );

  if (!currentOffer) return null;

  return (
    <Dialog open={true} onOpenChange={handleOpenChange}>
      {/* 关闭按钮与「点外部关闭」都恢复了：关闭现在只是「待会儿再说」，不再作废传输。
          此前两者被堵死是与「关闭 = 拒绝」配套的防误触措施——那条语义一改，它们就从
          保护变成了牢笼（唯一出口是 Esc，而 Esc 恰恰是最容易误按的那个）。 */}
      <DialogContent
        data-testid="transfer-offer-dialog"
        className="flex max-h-[min(820px,calc(100dvh-2rem))] flex-col gap-0 overflow-hidden rounded-[20px] p-0 sm:max-w-2xl"
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

        <DialogFooter className="shrink-0 flex-row items-center gap-3 border-t border-border/60 bg-muted/20 px-5 py-4 sm:justify-end">
          {/* 「还有几条在等」——契约明文要求（DESIGN.md Incoming Request Contract 的
              Queue, don't stack）。没有它，处理完一条会突然又弹一个一模一样的框，
              用户无从知道这是队列还是重复弹窗。aria-live 让读屏也听得到。 */}
          {waitingCount > 0 && (
            <span
              aria-live="polite"
              className="mr-auto text-xs text-muted-foreground"
            >
              <Trans>还有 {waitingCount} 条在等待</Trans>
            </span>
          )}
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
