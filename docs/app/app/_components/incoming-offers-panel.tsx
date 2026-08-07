"use client";

// 待处理的入站文件请求——**决策**，不是结果。
//
// 从 `receive-panel.tsx` 抽出来的：那个文件里同时住着这块与收件箱列表，而
// `app/app/inbox/page.tsx` 自己就论证过两者的分工——「待处理请求是**决策**（有时效），
// 收件箱是**结果**（可回看）」。既然分工写下来了，文件也该分开。
//
// 请求到达时先弹全局对话框（`transfer-offer-host.tsx`，挂 layout 所以任何路由下都看得见）；
// 这里是**关掉之后**的回看入口。DESIGN.md 的 Incoming Request Contract：文件 offer 的关闭
// **≠** 拒绝，所以必须有地方能把它找回来——就是这里。

import { Trans } from "@lingui/react/macro";
import { formatFileSize } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import { WebErrorCard } from "./web-error-view";
import { getNode } from "../_lib/node-runtime";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";

/**
 * 每条 offer 最多列几个文件名，其余折成「还有 N 个」——offer 可能带上百个文件。
 *
 * **这里刻意不用 `FileBrowser`**（全局对话框用了）。两处的问题不同：对话框回答「这一堆
 * 到底是什么，我收不收」，需要完整结构与体量；本面板是多条 offer 的**索引**，每条只要
 * 认得出「这是谁发的哪一堆」，真要细看就去开对话框。给每条都塞一个带视图切换的浏览器，
 * 会让「接受 / 拒绝」两个按钮——这张卡存在的理由——被推到屏幕外。
 */
const OFFER_FILE_PREVIEW_LIMIT = 5;

export function IncomingOffersPanel() {
  const offers = useWebNode((s) => s.offers);
  const decideAction = useKeyedAsyncAction();

  const offerList = Object.values(offers);

  const decide = (sessionId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    void decideAction.run(sessionId, async () => {
      if (accept) await node.accept_offer(sessionId);
      else await node.reject_offer(sessionId);
      webNodeActions.removeOffer(sessionId);
    });
  };

  // **空的时候整块不渲染。**
  //
  // 这是一块**有时效的决策区**，不是常驻清单：没有待处理请求时，一整张卡加一句
  // 「暂无待处理的入站文件请求」只是在收件箱顶部常年占掉一屏的三分之一，把真正的内容
  // （已收到的文件）推下去。而它非空时又必须显眼——两者不冲突，条件渲染同时满足。
  //
  // 与「设备网格空态要教学」不同的地方在于：那里的空是用户能改变的（去配对），
  // 这里的空是**正常状态**（没人正在给你发东西），没有什么可教。
  if (offerList.length === 0) return null;

  return (
    <div className="rounded-[var(--radius-panel-sm)] border border-[var(--brand-solid)]/30 bg-card p-4 shadow-xs sm:p-6">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>待处理请求</Trans>
        </h2>
        <p
          className="rounded-full bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground"
          role="status"
          aria-live="polite"
        >
          <Trans>{offerList.length} 个待处理</Trans>
        </p>
      </div>

      <>
        {/* 请求到达时会先弹全局对话框（`transfer-offer-host.tsx`）。这里是**关掉之后**
            的回看入口——说清楚，否则同一件事出现在两个地方会让人以为是重复。 */}
        <p className="mt-2 text-xs text-muted-foreground">
          <Trans>你关掉提示后，请求会留在这里等你决定。</Trans>
        </p>
        {/* 限高自滚：这块住在收件箱页主从布局的**上方**，而那一页不整页滚（`fill`）。
            请求条数没有上限（一次配对下来堆三五条很常见），不限高的话它会把下面「已收到的
            文件」挤到接近 0 高——那块于是整个够不着。上限取 38dvh 与 340px 的较小者：
            前者让它在大屏上也只占约三分之一，后者防止在超高视口里长成一面墙。 */}
        <ul className="mt-3 max-h-[min(38dvh,340px)] space-y-2 overflow-y-auto">
          {offerList.map((offer) => {
            const rest = offer.files.length - OFFER_FILE_PREVIEW_LIMIT;
            return (
            <li key={offer.sessionId} className="rounded-lg border bg-background px-3 py-2">
              <p className="text-xs text-foreground">
                <Trans>
                  <span className="font-medium">{offer.deviceName}</span> 想发送{" "}
                  {offer.files.length} 个文件（{formatFileSize(offer.totalSize)}）
                </Trans>
              </p>
              <ul className="mt-1 space-y-0.5">
                {offer.files.slice(0, OFFER_FILE_PREVIEW_LIMIT).map((f) => (
                  <li key={f.fileId} className="truncate font-mono text-[11px] text-muted-foreground">
                    {f.name}（{formatFileSize(f.size)}）
                  </li>
                ))}
                {rest > 0 && (
                  <li className="text-[11px] text-muted-foreground">
                    <Trans>还有 {rest} 个文件</Trans>
                  </li>
                )}
              </ul>
              <div className="mt-2 flex gap-2">
                <Button
                  size="sm"
                  onClick={() => decide(offer.sessionId, true)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="min-h-9"
                >
                  <Trans>接受</Trans>
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => decide(offer.sessionId, false)}
                  disabled={decideAction.isPending(offer.sessionId)}
                  className="min-h-9"
                >
                  <Trans>拒绝</Trans>
                </Button>
              </div>
            </li>
            );
          })}
        </ul>
      </>
      {decideAction.latestError && <WebErrorCard error={decideAction.latestError} className="mt-2 text-xs" />}
    </div>
  );
}
