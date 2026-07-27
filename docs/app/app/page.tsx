import { ConnectionPanel } from "./_components/connection-panel";
import { DevEventLog } from "./_components/dev-event-log";
import { DeviceList } from "./_components/device-list";
import { NodePanel } from "./_components/node-panel";
import { PairingPanel } from "./_components/pairing-panel";
import { ReceivePanel } from "./_components/receive-panel";
import { SendPanel } from "./_components/send-panel";
import { TransferActivityPanel } from "./_components/transfer-activity-panel";
import { WebErrorView } from "./_components/web-error-view";

// 首屏：节点在本页自动启动，身份 / 连接 / 配对 / 发送 / 传输活动 / 接收各占一块。
// 面板顺序即用户路径——先有身份和连接，才谈得上配对，配对后才能收发。
export default function AppPage() {
  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-base font-semibold text-fd-foreground">浏览器传输端</h1>
        <p className="mt-1 text-sm text-fd-muted-foreground">
          节点已在本页启动，与桌面 / 移动端同源。已配对设备、收件箱与传输历史刷新后仍在。
        </p>
      </div>
      <WebErrorView />
      <NodePanel />
      <ConnectionPanel />
      <PairingPanel />
      <DeviceList />
      <SendPanel />
      <TransferActivityPanel />
      <ReceivePanel />
      <DevEventLog />
    </div>
  );
}
