"use client";

// 文本确认和文件 offer 都是跨路由的入站决策；挂在 app layout，避免用户停留在发送页时看不见。
import { Trans, useLingui } from "@lingui/react/macro";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import type { PendingTextDeliverySummary } from "../_lib/view-types";

export function TextDeliveryAttentionHost() {
  const status = useWebNode((state) => state.status);
  const revision = useWebNode((state) => state.textDeliveryRevision);
  const [pendingTexts, setPendingTexts] = useState<
    PendingTextDeliverySummary[]
  >([]);

  const refresh = useCallback(async () => {
    const node = getNode();
    if (!node) return;
    try {
      setPendingTexts(
        (await node.pending_text_deliveries()) as PendingTextDeliverySummary[],
      );
    } catch (error) {
      // 临时 wasm 调用失败不应把已展示的确认框清空。
      console.error("[web] pending text deliveries refresh failed", error);
    }
  }, []);

  useEffect(() => {
    if (status !== "running") return;
    void refresh();
    const timer = window.setInterval(() => void refresh(), 30_000);
    return () => window.clearInterval(timer);
  }, [refresh, revision, status]);

  const pending = pendingTexts[0] ?? null;
  if (!pending) return null;

  return (
    <PendingTextConfirmation
      pending={pending}
      onRespond={async (accepted) => {
        const node = getNode();
        if (!node) return;
        await node.confirm_text_delivery(pending.deliveryId, accepted);
        setPendingTexts((items) =>
          items.filter((item) => item.deliveryId !== pending.deliveryId),
        );
        if (accepted) webNodeActions.refreshInbox();
        await refresh();
      }}
    />
  );
}

function PendingTextConfirmation({
  pending,
  onRespond,
}: {
  pending: PendingTextDeliverySummary;
  onRespond: (accepted: boolean) => Promise<void>;
}) {
  const { t } = useLingui();
  const [responding, setResponding] = useState(false);
  const respond = async (accepted: boolean) => {
    if (responding) return;
    setResponding(true);
    try {
      await onRespond(accepted);
    } catch (error) {
      toast.error(t`操作失败，请重试`);
      console.error("[web] confirm text delivery failed", error);
    } finally {
      setResponding(false);
    }
  };

  return (
    <AlertDialog open>
      <AlertDialogContent data-testid="text-delivery-confirmation">
        <AlertDialogHeader>
          <AlertDialogTitle><Trans>接收文本</Trans></AlertDialogTitle>
          <AlertDialogDescription>
            <Trans>{pending.peerName} 想向你发送一段文本。</Trans>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="max-h-72 overflow-y-auto whitespace-pre-wrap break-words rounded-xl bg-muted/50 p-3 text-sm leading-6 text-foreground">
          {pending.body}
        </div>
        <AlertDialogFooter>
          <Button type="button" variant="outline" disabled={responding} onClick={() => void respond(false)}>
            <Trans>拒绝</Trans>
          </Button>
          <Button type="button" disabled={responding} onClick={() => void respond(true)}>
            {responding ? <Trans>处理中…</Trans> : <Trans>接收</Trans>}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
