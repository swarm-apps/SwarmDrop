## 1. 拆分 devices 页

- [ ] 1.1 抽 `-components/add-device-section.tsx`(配对码区、附近设备行、`PairingInputDialog`),保留 v0.6.0 的 `phase==='success'` 自动关闭逻辑
- [ ] 1.2 `index.lazy.tsx` 改为只编排 `HomeOverview`/`PairedDevicesSection`/`ActiveTransfersSection`/`AddDeviceSection`
- [ ] 1.3 `tsc --noEmit` 绿,手动验证配对/发送/解绑流程不回归

## 2. ambient 背景

- [ ] 2.1 合并签名默认值与 `CONFIG` 为单一配置来源
- [ ] 2.2 不可见/失焦时暂停 WebGL(`IntersectionObserver` + `visibilitychange`),恢复续跑 RAF
- [ ] 2.3 目视确认暂停/恢复无视觉回归

## 3. start 参数去重 + specta

- [ ] 3.1 删 `lifecycle.rs::start` 的 `custom_bootstrap_nodes` 位置参与 legacy 合并逻辑
- [ ] 3.2 重生成 tauri-specta `src/lib/bindings.ts`(确认导出机制后运行)
- [ ] 3.3 `network-store.ts` 第二位置参不再传,`tsc` + `vitest` 绿

## 4. 测试覆盖

- [ ] 4.1 `e2e_lan_helper`:无私网 IP 时改 `#[ignore]` 或显式跳过,杜绝静默 PASS(零成本先落地)
- [ ] 4.2 清除 maestro smoke 中 `when visible→assertVisible 同元素` 的永真断言
- [ ] 4.3 为 device-policy/inbox-delete 提供"预置已配对设备/收件箱数据"夹具,使被网关包住的断言真正运行;无法预置者显式标注条件覆盖
