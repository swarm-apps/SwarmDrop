import { ConnectionPanel } from "./_components/connection-panel";
import { DevEventLog } from "./_components/dev-event-log";
import { DeviceList } from "./_components/device-list";
import { NodePanel } from "./_components/node-panel";
import { PairingPanel } from "./_components/pairing-panel";
import { ReceivePanel } from "./_components/receive-panel";
import { SendPanel } from "./_components/send-panel";
import { TransferActivityPanel } from "./_components/transfer-activity-panel";
import { WebErrorView } from "./_components/web-error-view";

// 首屏（基座 + ①②③④⑤）：节点已在本页自动启动，展示身份/状态/连接/配对/发送/接收。统一的
// 传输活动视图（进度/速率/断点续传）归 ⑥（#80）。
export default function AppPage() {
  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-base font-semibold text-fd-foreground">浏览器传输端</h1>
        <p className="mt-1 text-sm text-fd-muted-foreground">
          节点已在本页启动，与桌面 / 移动端同源。统一的传输活动视图将在后续模块接入。
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
