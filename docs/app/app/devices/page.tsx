import type { Metadata } from "next";
import { DeviceList } from "../_components/device-list";
import { PageHeader } from "../_components/page-header";
import { PairingPanel } from "../_components/pairing-panel";
import { NAV } from "../_lib/nav";

export const metadata: Metadata = { title: NAV.devices.label };

// 应用首页（`/app` 重定向到这里）：设备关系页，承接桌面端 `_app/devices` 的角色——
// 先有已配对设备，发送、收件、传输才谈得上。
export default function DevicesPage() {
  return (
    <>
      <PageHeader item={NAV.devices} />
      <DeviceList />
      <PairingPanel />
    </>
  );
}
