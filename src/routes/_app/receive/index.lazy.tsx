/**
 * Receive Page (Lazy)
 * 接收文件页面 —— 纯展示页面,提示等待对方发送文件
 *
 * Offer 的消费统一由 TransferOfferDialog 处理(全局弹窗),
 * 此页面不再自动导航或消费 pendingOffers。
 */

import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { Activity, Download, Inbox, ShieldCheck } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import {
  CommandDock,
  GlassPanel,
  InfoTile,
  TaskButton,
  TaskContent,
  TaskHeroPanel,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/receive/")({
  component: ReceivePage,
});

function ReceivePage() {
  const navigate = useNavigate();

  const handleBack = () => {
    navigate({ to: "/transfer" });
  };

  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>接收文件</Trans>} onBack={handleBack} />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <GlassPanel className="min-h-[420px]">
            <div className="flex h-full flex-col items-center justify-center gap-6 p-6 text-center">
              <div className="glass-accent flex size-18 items-center justify-center rounded-[28px] text-brand">
                <Download className="size-8" />
              </div>
              <div>
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>等待对方发送文件</Trans>
                </h1>
                <p className="mt-2 max-w-[42ch] text-sm leading-6 text-muted-foreground">
                  <Trans>有设备发起传输时，SwarmDrop 会弹出确认窗口。</Trans>
                </p>
              </div>
              <div className="grid w-full max-w-md gap-2 sm:grid-cols-2">
                <InfoTile
                  icon={Inbox}
                  label={<Trans>接收完成后</Trans>}
                  value={<Trans>进入收件箱</Trans>}
                />
                <InfoTile
                  icon={Activity}
                  label={<Trans>暂停或失败</Trans>}
                  value={<Trans>保留在活动记录</Trans>}
                />
              </div>
            </div>
          </GlassPanel>

          <TaskHeroPanel
            icon={ShieldCheck}
            label={<Trans>接收策略</Trans>}
            title={<Trans>可信设备可减少确认</Trans>}
            description={<Trans>已配对设备可以在设备策略中设置接收方式和保存位置。</Trans>}
          >
            <div className="grid content-end gap-2">
              <InfoTile
                label={<Trans>确认入口</Trans>}
                value={<Trans>全局传输弹窗</Trans>}
              />
              <InfoTile
                label={<Trans>完成内容</Trans>}
                value={<Trans>收件箱统一管理</Trans>}
              />
            </div>
          </TaskHeroPanel>
        </div>

        <CommandDock>
          <TaskButton variant="outline" onClick={() => navigate({ to: "/inbox" })}>
            <Inbox className="size-4" />
            <Trans>打开收件箱</Trans>
          </TaskButton>
          <TaskButton onClick={() => navigate({ to: "/transfer" })}>
            <Activity className="size-4" />
            <Trans>查看活动</Trans>
          </TaskButton>
        </CommandDock>
      </TaskContent>
    </TaskPageShell>
  );
}
