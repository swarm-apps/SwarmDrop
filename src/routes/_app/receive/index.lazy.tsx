/**
 * Receive Page (Lazy)
 * 接收文件页面 —— 纯展示页面,提示等待对方发送文件
 *
 * Offer 的消费统一由 TransferOfferDialog 处理(全局弹窗),
 * 此页面不再自动导航或消费 pendingOffers。
 */

import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowLeft, Download } from "lucide-react";
import { Trans } from "@lingui/react/macro";

export const Route = createLazyFileRoute("/_app/receive/")({
  component: ReceivePage,
});

function ReceivePage() {
  const navigate = useNavigate();

  const handleBack = () => {
    navigate({ to: "/transfer" });
  };

  return (
    <main className="flex h-full flex-1 flex-col bg-background">
      <header className="flex h-13 items-center gap-2 border-b border-border px-4 lg:px-5">
        <button
          type="button"
          onClick={handleBack}
          className="flex size-8 items-center justify-center rounded-md hover:bg-muted"
        >
          <ArrowLeft className="size-4" />
        </button>
        <h1 className="text-[15px] font-medium text-foreground">
          <Trans>接收文件</Trans>
        </h1>
      </header>
      <div className="flex flex-1 flex-col items-center justify-center gap-3">
        <div className="flex size-14 items-center justify-center rounded-full bg-muted">
          <Download className="size-7 text-muted-foreground" />
        </div>
        <div className="flex flex-col items-center gap-1 text-center">
          <p className="text-sm font-medium text-foreground">
            <Trans>等待对方发送文件</Trans>
          </p>
          <p className="text-xs text-muted-foreground">
            <Trans>当有设备向你发送文件时,会弹出确认对话框</Trans>
          </p>
        </div>
      </div>
    </main>
  );
}
