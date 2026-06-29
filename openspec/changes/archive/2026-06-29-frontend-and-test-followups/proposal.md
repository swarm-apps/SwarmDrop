## Why

v0.6.0 审查留下两类非阻断遗留:(1) 桌面前端技术债——`devices/index.lazy.tsx` 膨胀到 ~900 行把配对 UI 也吸进设备页(god component)、`app-ambient-background.tsx` 有两套互相冲突的配置且 WebGL 动画长驻不失焦暂停(耗电)、`start` 命令仍带已被 `network_options` 取代的 legacy `custom_bootstrap_nodes` 位置参且前端重复传;(2) 测试假绿灯——`device-policy`/`inbox-delete` 等 maestro 用例的核心断言被 `runFlow when <数据态>` 网关包住,而单设备 fresh-state 下这些数据态(device-card-0/inbox-list)永不出现,CI 里实际零覆盖只截图;`e2e_lan_helper` 在无私网 IP 时静默 return 仍报 PASS。

## What Changes

- **拆分 `devices/index.lazy.tsx`**:把内联配对(`AddDeviceSection`/`PairingInputDialog`/附近设备行)抽到独立组件文件,设备页主文件只编排区块。
- **合并 ambient 背景配置**:去掉签名默认值与 `CONFIG` 的双重来源(单一真相);WebGL 动画在页面不可见/失焦时暂停(`IntersectionObserver` + `visibilitychange`)。
- **去重 `start` 命令参数**:删除 legacy `custom_bootstrap_nodes` 位置参,重生成 tauri-specta `bindings.ts`,前端 `network-store.ts` 只传 `network_options`。
- **新增有效测试覆盖**(新能力 `meaningful-transfer-test-coverage`):为 maestro 信任策略/收件箱用例提供可触达数据态的多设备或预置数据夹具(或显式标注为条件覆盖);`e2e_lan_helper` 无私网 IP 时显式 `#[ignore]`/失败而非静默 PASS。

## Capabilities

### New Capabilities
- `meaningful-transfer-test-coverage`: 信任策略/收件箱/LAN helper 的自动化用例 SHALL 在 CI 默认环境下真正执行其行为断言(而非被无法满足的数据态网关静默跳过);环境受限的用例 SHALL 显式跳过而非伪装通过。

### Modified Capabilities
<!-- 前端拆分/ambient/start 去重均为重构与实现细节,无 spec 级需求变更。 -->

## Impact

- 桌面前端:`src/routes/_app/devices/index.lazy.tsx` 拆分、`src/components/layout/app-ambient-background.tsx`、`src/stores/network-store.ts`。
- 桌面后端:`src-tauri/src/commands/lifecycle.rs`(start 签名)+ tauri-specta 重生成 `src/lib/bindings.ts`。
- 测试:`.maestro/smoke/*.yaml`(RN 仓)、`crates/core/tests/e2e_lan_helper.rs`、新增测试夹具/预置数据脚本。
