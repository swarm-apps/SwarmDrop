## 1. 拆分 devices 页

- [x] 1.1 抽 `-components/add-device-section.tsx`（配对码区、附近设备行、`PairingInputDialog`），保留 v0.6.0 的 `phase==='success'` 自动关闭逻辑（原样迁移）；另抽 `-components/section-primitives.tsx`（`SectionHeader`/`SectionShell`/`EmptyPanel`）避免循环依赖
- [x] 1.2 `index.lazy.tsx` 改为只编排 `HomeOverview`/`PairedDevicesSection`/`ActiveTransfersSection`/`AddDeviceSection`，清理 unused imports
- [x] 1.3 `tsc --noEmit` 绿（`noUnusedLocals`/`noUnusedParameters` 开启）；handler 未改，逻辑结构一致。**需手动验证**配对/发送/解绑流程不回归

## 2. ambient 背景

- [x] 2.1 合并签名默认值与 `CONFIG` 为单一配置来源（props 仅留主题覆盖项；补回原先静默来自签名默认的 `noiseFrequency: 2.35`，渲染值不变）
- [x] 2.2 不可见/失焦时暂停 WebGL（`IntersectionObserver` + `visibilitychange` 双触发），恢复续跑 RAF（暂停时长累加并从时间戳扣除，避免相位跳变）
- [~] 2.3 目视确认暂停/恢复无视觉回归——**需手动视觉 QA**（这是本 change 唯一的可见行为变化）

## 3. start 参数去重 + specta

- [x] 3.1 删 `commands/lifecycle.rs::start` 的 `custom_bootstrap_nodes` 位置参与 legacy 合并逻辑（确认冗余：唯一调用方已在 `network_options` 内传 `customBootstrapNodes`，合并恒为 no-op）；顺带修 `identity.rs` 一处过期 doc
- [x] 3.2 经 `cargo test -p swarmdrop --test specta_export` 重生成 `src/lib/bindings.ts`（diff 仅限 `start` 签名 + 该 doc）
- [x] 3.3 `network-store.ts` 改 `commands.start(pairedDevices, networkOptions)` 不再传第二位置参；更新 `network-store.test.ts`；`tsc` + `vitest` 绿

## 4. 测试覆盖

- [x] 4.1 `e2e_lan_helper`：两个用例整体依赖可绑定私网 IPv4，加 `#[ignore]`（默认套件不计入，杜绝静默 PASS），无 IP 早退由 `return` 改 `panic!`（`--ignored` 手动跑时显式失败）。验证 `0 passed; 2 ignored`
- [x] 4.2 清除 maestro smoke 中 `when visible→assertVisible 同元素` 的永真断言（`mobile-foundation`/`device-policy`/`inbox-delete-confirmation` 各一处空态自断言已删）
- [~] 4.3 device-policy/inbox-delete 等数据态门控块已加显式"条件覆盖"注释（满足 spec 的"无法预置者显式标注条件覆盖"）；**完整 secret-store/预置数据夹具按 design 的"后续"保留**
