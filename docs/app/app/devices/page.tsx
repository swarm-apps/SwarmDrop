import type { Metadata } from "next";
import { DeviceGrid } from "../_components/device-grid";
import { PageHeader } from "../_components/page-header";
import { PairingPanel } from "../_components/pairing-panel";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.devices) };

// 应用首页（`/app` 重定向到这里）：设备关系页，承接桌面端 `_app/devices` 的角色——
// 先有已配对设备，发送、收件、传输才谈得上。
//
// ## 版式：设备在前，配对在后
//
// 设备网格是主体，配对是**次级区块**。理由是使用频率差两个数量级：配对是一次性动作
// （配完即长期信任），而设备清单是每次进来都要看的东西。
//
// ## 刻意不引入的两块
//
// 桌面端设备页还有「附近未配对设备」与节点启停 sheet，这里都不做：前者依赖 mDNS 局域网发现
// （浏览器没有这个能力），后者在 Web 上由 `WebNodeBootstrap` 自动接管，给用户一个开关只会
// 让人以为自己需要管它。
export default function DevicesPage() {
  return (
    <>
      <PageHeader nav="devices" />
      <DeviceGrid />
      <PairingPanel />
    </>
  );
}
