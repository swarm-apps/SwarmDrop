import { ConnectionPanel } from "./_components/connection-panel";
import { DevEventLog } from "./_components/dev-event-log";
import { NodePanel } from "./_components/node-panel";
import { WebErrorView } from "./_components/web-error-view";

// 首屏（基座 + ①②）：节点已在本页自动启动，展示身份/状态/连接。配对/收发归后续模块（#77~#80）。
export default function AppPage() {
  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-base font-semibold text-fd-foreground">浏览器传输端</h1>
        <p className="mt-1 text-sm text-fd-muted-foreground">
          节点已在本页启动，与桌面 / 移动端同源。配对、收发将在后续模块接入。
        </p>
      </div>
      <WebErrorView />
      <NodePanel />
      <ConnectionPanel />
      <DevEventLog />
    </div>
  );
}
