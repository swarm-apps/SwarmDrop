/**
 * 文本接收确认的全局宿主。
 *
 * 请求不是收件箱页专属的内容：用户正在发送或浏览设备时也必须能作出决定。
 * 待确认正文仍只保留在内核的持久化队列中；这里仅在事件/恢复轮询后读取队首。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "@tanstack/react-router";
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
import { commands, type PendingTextDeliverySummary } from "@/lib/bindings";
import { getErrorMessage } from "@/lib/errors";
import { useInboxStore } from "@/stores/inbox-store";

export function TextDeliveryAttentionHost() {
  const navigate = useNavigate();
  // action 走 selector 取（项目规则，`check:zustand-access` 规则 A 看守）：它是 store 内的
  // 稳定引用，订阅它不会带来额外重渲染，因此没有理由为它开一条读快照的例外。
  // 注：那个检查按源码文本匹配，连注释里都不能把那个调用形态原样写出来。
  const loadItems = useInboxStore((state) => state.loadItems);
  const [pendingTexts, setPendingTexts] = useState<
    PendingTextDeliverySummary[]
  >([]);
  const routeToInboxOnFocus = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setPendingTexts(await commands.pendingTextDeliveries());
    } catch (error) {
      // 失败时保留上一帧，避免一次短暂 IPC 异常把待确认请求从用户眼前抹掉。
      console.error("[text-delivery-attention] refresh failed", error);
    }
  }, []);

  useEffect(() => {
    void refresh();
    let alive = true;
    let unlisten: (() => void) | undefined;
    void listen<{
      kind: "confirmation_required" | "received";
      peerName: string;
    }>("text-delivery-attention-received", (event) => {
      void refresh();
      // desktop notification plugin 没有桌面端点击回调；系统点击会先令窗口重新获焦，
      // 因而把已在后台收到的文本记为一次路由意图，在该激活时再打开收件箱。
      if (!document.hasFocus()) routeToInboxOnFocus.current = true;
      if (event.payload.kind === "received") {
        toast.info(t`收到来自 ${event.payload.peerName} 的文本`);
      }
    }).then((dispose) => {
      if (alive) unlisten = dispose;
      else dispose();
    });
    // 事件是主路径；此处只负责应用恢复、桥接漏事件时的可恢复性。
    const timer = window.setInterval(() => void refresh(), 30_000);
    return () => {
      alive = false;
      unlisten?.();
      window.clearInterval(timer);
    };
  }, [refresh]);

  useEffect(() => {
    const routeOnFocus = () => {
      if (!routeToInboxOnFocus.current) return;
      routeToInboxOnFocus.current = false;
      void navigate({ to: "/inbox" });
    };
    window.addEventListener("focus", routeOnFocus);
    return () => window.removeEventListener("focus", routeOnFocus);
  }, [navigate]);

  const pending = pendingTexts[0] ?? null;
  if (!pending) return null;

  return (
    <PendingTextConfirmation
      pending={pending}
      onRespond={async (accepted) => {
        await commands.confirmTextDelivery(pending.deliveryId, accepted);
        setPendingTexts((items) =>
          items.filter((item) => item.deliveryId !== pending.deliveryId),
        );
        if (accepted) await loadItems(false);
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
  const [responding, setResponding] = useState(false);
  const respond = async (accepted: boolean) => {
    if (responding) return;
    setResponding(true);
    try {
      await onRespond(accepted);
    } catch (error) {
      toast.error(getErrorMessage(error));
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
