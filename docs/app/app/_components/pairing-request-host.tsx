"use client";

// 入站配对请求的全局宿主 —— 挂在 layout，任何路由下都会弹。
//
// **为什么必须是全局的**：Web 应用区是多路由（devices / send / inbox / transfer / settings）。
// 此前这块是内联在设备页 `pairing-panel.tsx` 里的一段列表，于是「用户正在 /app/send 挑文件时
// 对方发来配对请求」= 他完全看不见，除非碰巧切回设备页。桌面端 (`src/routes/_app.tsx` 的
// `ConnectionRequestDialog`) 与移动端 (`app/_layout.tsx` 的 `PairingRequestHost`) 早就是全局
// 宿主，Web 是三端里唯一的分叉。
//
// 队列语义：一次只弹队首，处理完自动接上下一个——与移动端同名组件一致。

import { Trans, useLingui } from "@lingui/react/macro";
import { ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { getNode, refreshPairedDevices } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { WebErrorCard } from "./web-error-view";

export function PairingRequestHost() {
  const { t } = useLingui();
  const pendingPairings = useWebNode((s) => s.pendingPairings);
  const respond = useAsyncAction();

  // 队首即当前。**不做 dismiss**：配对请求是即时决策，关掉就是拒绝——
  // 留一个「稍后再说」只会让对方一直卡在等待里。
  const current = pendingPairings[0] ?? null;

  const decide = (accept: boolean) => {
    if (!current) return;
    const node = getNode();
    if (!node) return;
    const pendingId = current.pendingId;
    respond.run(
      () => node.respond_pairing_request(pendingId, accept),
      (persisted) => {
        webNodeActions.removePendingPairing(pendingId);
        // 接受后设备清单要立刻认识它，否则要等下一轮轮询才出现在设备页
        if (accept) refreshPairedDevices(node);
        // 「一半成功」：对端已经认了本机、本页也能用，但记录没写进 IndexedDB。
        // 报成失败会和对端的认知分叉，静默略过则用户不知道自己还得再配一次。
        if (accept && !persisted) {
          toast.warning(t`配对成功，但这条记录没能存进浏览器`, {
            description: t`刷新页面后需要重新配对。`,
          });
        }
      },
    );
  };

  return (
    <Dialog
      open={current !== null}
      onOpenChange={(open) => {
        // 点遮罩 / Esc = 拒绝，与桌面端 ConnectionRequestDialog 同语义。
        // 在途请求不打断——否则一次误触会让「已发出的接受」和「随后的拒绝」抢跑。
        if (!open && !respond.pending) decide(false);
      }}
    >
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader className="items-center gap-2 text-center sm:text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-[var(--brand-solid)]/12">
            <ShieldCheck className="size-6 text-brand" aria-hidden />
          </div>
          <DialogTitle>
            <Trans>配对请求</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>
              <span className="font-medium text-foreground">{current?.deviceName}</span> 想与这台设备配对
            </Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="rounded-lg bg-muted px-3 py-2.5">
          <p className="text-xs text-muted-foreground">
            <Trans>对方节点 ID</Trans>
          </p>
          <p className="mt-0.5 font-mono text-xs break-all text-foreground">{current?.peerId}</p>
        </div>

        <p className="text-xs text-muted-foreground">
          <Trans>配对之后双方都能互发文件。确认这是你认识的设备再接受。</Trans>
        </p>

        {respond.error && <WebErrorCard error={respond.error} className="text-xs" />}

        <DialogFooter className="gap-2 sm:gap-2">
          <Button
            variant="outline"
            onClick={() => decide(false)}
            disabled={respond.pending}
            className="flex-1"
          >
            <Trans>拒绝</Trans>
          </Button>
          <Button
            onClick={() => decide(true)}
            disabled={respond.pending}
            className="flex-1"
            data-testid="pairing-accept"
          >
            {respond.pending ? t`处理中…` : t`接受`}
          </Button>
        </DialogFooter>

        {pendingPairings.length > 1 && (
          <p className="text-center text-xs text-muted-foreground">
            <Trans>还有 {pendingPairings.length - 1} 个请求等待处理</Trans>
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}
