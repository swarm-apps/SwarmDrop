## Context

非阻断的 v0.6.0 审查遗留,集中在桌面前端可维护性与测试可信度。功能行为已正确,这里只动结构与测试。

## Goals / Non-Goals

**Goals:**
- 设备页可读性:配对 UI 与设备编排解耦。
- ambient 背景:单一配置来源 + 不可见时停 WebGL(省电)。
- `start` 命令去掉冗余 legacy 参数,specta 类型对齐。
- 测试不再假绿灯:断言要么真执行,要么显式跳过。

**Non-Goals:**
- 不改任何用户可见功能行为。
- 不改 data-channel/projection 等核心逻辑。

## Decisions

1. **devices 拆分**:抽 `-components/add-device-section.tsx`(含配对码、附近设备、`PairingInputDialog`),`index.lazy.tsx` 只保留页面骨架 + 区块组合。配对成功关闭逻辑(v0.6.0 已修)随组件迁移,保持其 `phase==='success'` 自动关闭。
2. **ambient 配置单一真相**:删掉组件签名上与 `CONFIG` 重复的默认值,只留一处常量;props 仅覆盖少量主题相关项。WebGL 暂停用 `IntersectionObserver`(组件不在视口)+ `document.visibilitychange`(标签页隐藏)双触发,恢复时续跑 RAF。备选(仅 visibilitychange)漏掉"被其他面板覆盖但标签页仍可见"的场景。
3. **start 参数去重**:删 `custom_bootstrap_nodes` 位置参 → `cargo build` 触发 tauri-specta 导出测试重生成 `bindings.ts`(确认导出机制:`cargo test` 的 export 测试)→ 改 `network-store.ts` 第二位置参不再传。
4. **测试覆盖**:
   - maestro:提供"预置一台已配对设备"的夹具(env 注入或启动前写 secret-store),使 `device-card-0`/inbox 数据态可达,让被网关包住的断言真正运行;无法预置的用例显式标注条件覆盖。
   - `e2e_lan_helper`:无私网 IP 时改 `#[ignore]`(默认不跑,`--ignored` 手动跑)或返回前 `panic!`/标记跳过,杜绝静默 PASS。

## Risks / Trade-offs

- [ambient WebGL 暂停引入视觉/动画回归] → 目视确认暂停/恢复无闪烁;保留可关闭开关。
- [specta 重生成产物漂移] → 仅 diff `start` 签名相关部分,其余应不变。
- [maestro 夹具增加 CI 复杂度] → 夹具可选;先把 `e2e_lan_helper` 显式跳过(零成本)落地,maestro 夹具作为后续。

## Open Questions

- maestro 多设备夹具用 Maestro Cloud 多设备还是单设备预置数据?默认单设备预置 secret-store 数据,成本最低。
