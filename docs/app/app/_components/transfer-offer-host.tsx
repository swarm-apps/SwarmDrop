"use client";

// 入站文件请求的全局宿主 —— 挂在 layout，任何路由下都会弹。
//
// 与 `pairing-request-host.tsx` 同样的理由：Web 应用区是多路由，此前这块只内联在
// 收件箱页，用户在别的页面时收到请求完全看不见。桌面 `TransferOfferDialog` 与移动
// `TransferOfferHost` 都是全局宿主。
//
// ## 与收件箱那份列表的分工（两处都要留，不是重复）
//
// 这里回答「现在决定」，收件箱的「待处理请求」回答「还没决定的都在这」。**dismiss 是
// 两者必须并存的原因**：本对话框允许关掉不决策（用户可能正在做别的事），关掉之后 offer
// 仍然有效、仍在等待，收件箱那份列表就是它唯一的回看入口。
//
// 桌面端没有这个回看入口（dismiss 之后只能等 offer 超时），那是它的既有缺口——
// Web 多路由形态下补上它是改善，不是要向桌面看齐的分叉。
//
// 落盘位置这里**不给选**：浏览器写 OPFS，没有「保存到哪个文件夹」可言（那是桌面端
// TransferOfferDialog 才有的一步）。接收后的文件在收件箱里下载。

import { Trans, useLingui } from "@lingui/react/macro";
import { Download } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { formatFileSize } from "@swarmdrop/shared-view";
import { FileBrowser } from "@swarmdrop/file-browser";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { peerLabel } from "../_lib/device-presentation";
import { itemsFromOffer } from "../_lib/file-browser-adapters";
import { getNode } from "../_lib/node-runtime";
import { preferencesActions, usePreferences } from "../_lib/preferences-store";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { WebErrorCard } from "./web-error-view";

export function TransferOfferHost() {
  const { t } = useLingui();
  const offers = useWebNode((s) => s.offers);
  const decide = useAsyncAction();
  const [dismissedSessionId, setDismissedSessionId] = useState<string | null>(null);

  const offerList = Object.values(offers);
  const first = offerList[0] ?? null;
  const current = first && first.sessionId !== dismissedSessionId ? first : null;

  // 被 dismiss 的那个 offer 一旦消失（已决策 / 超时 / 对端取消），标记要跟着清，
  // 否则下一次同 sessionId 复用时会被当成「用户已经关过了」而不弹。
  useEffect(() => {
    if (dismissedSessionId && !(dismissedSessionId in offers)) {
      setDismissedSessionId(null);
    }
  }, [dismissedSessionId, offers]);

  const respond = (accept: boolean) => {
    if (!current) return;
    const node = getNode();
    if (!node) return;
    const sessionId = current.sessionId;
    decide.run(
      () => (accept ? node.accept_offer(sessionId) : node.reject_offer(sessionId)),
      () => webNodeActions.removeOffer(sessionId),
    );
  };

  const files = current?.files ?? [];
  const view = usePreferences((s) => s.fileBrowserViews.transfer);
  const offerItems = useMemo(
    () => (current ? itemsFromOffer(current.sessionId, current.files) : []),
    [current],
  );
  // 队列里除当前这条之外还剩几条。DESIGN.md 的 Incoming Request Contract：
  // 「Queue, don't stack — 一次只显示一条，**多于一条时要说还剩几条**」。
  // 少了这句，处理完一条会突然又弹出一个一模一样的对话框，用户会以为自己刚才没点上。
  const queued = Math.max(0, offerList.length - 1);

  return (
    <Dialog
      open={current !== null}
      onOpenChange={(open) => {
        // **关掉 ≠ 拒绝**（与配对请求相反）：对方已经在等着传了，误触关掉就判拒绝
        // 太重。留在收件箱里等用户回来决定。
        if (!open && !decide.pending) setDismissedSessionId(current?.sessionId ?? null);
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader className="items-center gap-2 text-center sm:text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-[var(--brand-solid)]/12">
            <Download className="size-6 text-brand" aria-hidden />
          </div>
          <DialogTitle>
            <Trans>收到文件请求</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>
              <span className="font-medium text-foreground">
                {current ? peerLabel(current.deviceName, current.peerId) : ""}
              </span> 想发送{" "}
              {files.length} 个文件（{formatFileSize(current?.totalSize ?? 0)}）
            </Trans>
          </DialogDescription>
        </DialogHeader>

        {/* 三端共用的 `FileBrowser`（与桌面 `TransferOfferDialog` 同一形态）。
            此前是「列前 5 个 + 还有 N 个」——而这一屏正是用户决定收不收的唯一依据，
            对方发一整个文件夹时，那 5 行既读不出结构也说不清体量。
            高度钳在 clamp 里：对话框不能被一个几百文件的 offer 撑穿。 */}
        <FileBrowser
          items={offerItems}
          title={<Trans>文件</Trans>}
          view={view}
          onViewChange={(nextView) => preferencesActions.setFileBrowserView("transfer", nextView)}
          className="h-[clamp(240px,38vh,380px)] min-h-0 flex-none"
        />

        {queued > 0 && (
          <p className="text-center text-xs text-muted-foreground" role="status" aria-live="polite">
            <Trans>处理完这条后，还有 {queued} 条请求在等待</Trans>
          </p>
        )}

        {decide.error && <WebErrorCard error={decide.error} className="text-xs" />}

        <DialogFooter className="gap-2 sm:gap-2">
          <Button
            variant="outline"
            onClick={() => respond(false)}
            disabled={decide.pending}
            className="flex-1"
          >
            <Trans>拒绝</Trans>
          </Button>
          <Button
            onClick={() => respond(true)}
            disabled={decide.pending}
            className="flex-1"
            data-testid="offer-accept"
          >
            {decide.pending ? t`处理中…` : t`接受`}
          </Button>
        </DialogFooter>

        <p className="text-center text-xs text-muted-foreground">
          <Trans>关掉这个窗口不会拒绝——请求会留在收件箱等你决定。</Trans>
        </p>
      </DialogContent>
    </Dialog>
  );
}
