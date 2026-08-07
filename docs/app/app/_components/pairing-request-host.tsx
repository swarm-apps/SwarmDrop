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
import { useRef } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { getNode, refreshPairedDevices } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { PairingIdentityDialog } from "./pairing-identity-dialog";

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

  /**
   * 退场动画期间内容不能塌——同 `PairingConfirmDialog` 的 latch。此前这里读的是
   * `current?.deviceName`，队首被消费后 `current` 立刻变 null，而对话框还要淡出 ~150ms：
   * 那段时间里标题还在、设备名和 NodeId 已经空了。
   */
  const lastRef = useRef<typeof current>(null);
  if (current !== null) lastRef.current = current;
  const shown = current ?? lastRef.current;

  return (
    <PairingIdentityDialog
      open={current !== null}
      // 点遮罩 / Esc = 拒绝，与桌面端 ConnectionRequestDialog 同语义。**这里与出站那半刻意
      // 不同**：入站的对方正卡在等待里，不答复就是让他一直转圈，所以关闭必须是一个答复；
      // 出站关闭只是收起（那边关掉不会有人在等，但会毁掉邀请串的唯一副本）。
      // 在途请求不打断——否则一次误触会让「已发出的接受」和「随后的拒绝」抢跑。
      onOpenChange={(open) => {
        if (!open && !respond.pending) decide(false);
      }}
      icon={ShieldCheck}
      title={<Trans>配对请求</Trans>}
      description={
        <Trans>
          <span className="font-medium text-foreground">{shown?.deviceName}</span> 想与这台设备配对
        </Trans>
      }
      peerId={shown?.peerId}
      hint={<Trans>配对之后双方都能互发文件。确认这是你认识的设备再接受。</Trans>}
      error={respond.error}
      footer={
        <>
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
        </>
      }
      trailing={
        pendingPairings.length > 1 && (
          <p className="text-center text-xs text-muted-foreground">
            <Trans>还有 {pendingPairings.length - 1} 个请求等待处理</Trans>
          </p>
        )
      }
    />
  );
}
