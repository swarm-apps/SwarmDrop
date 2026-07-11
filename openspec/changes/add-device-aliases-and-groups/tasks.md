## 1. 本机组织数据与展示投影

- [x] 1.1 在 preferences store 定义版本兼容的 `deviceOrganization` 默认值、别名、分组和成员关系类型。
- [x] 1.2 实现别名、分组创建/重命名/排序/删除，以及设备多分组成员关系的 store 操作。
- [x] 1.3 实现共享设备展示投影：别名、对端名称、hostname、短 PeerId 的优先级和同名判定。
- [x] 1.4 在取消配对路径清理目标 PeerId 的别名和全部分组成员关系。
- [x] 1.5 为旧偏好、别名清空、多分组、删除分组和取消配对清理增加单元测试。

## 2. 设备页与发送目标体验

- [x] 2.1 为设备页增加“全部”“未分组”和用户分组筛选，保持在线设备优先排序。
- [x] 2.2 在设备操作菜单或详情中实现别名编辑及分组成员管理。
- [x] 2.3 实现分组管理入口，包括创建、重命名、排序和删除确认。
- [x] 2.4 在同名设备卡片和发送选择器展示分组与 `hostname · 短 PeerId` 次级身份信息。
- [x] 2.5 覆盖设备页筛选、同名消歧和离线已配对设备展示的组件测试。

## 3. MCP 设备组织投影

- [x] 3.1 在 Tauri MCP 层安全读取持久化的本机组织数据，并为旧格式或读取失败实现空组织 fallback。
- [x] 3.2 扩展 `list_paired_devices` 和 `list_available_devices` 输出 `displayName`、原始名称、分组和 `identityHint`。
- [x] 3.3 更新发送目标解析流程：多候选匹配时返回可消歧候选并要求用户确认，不直接调用发送。
- [x] 3.4 更新 MCP 指南，说明 PeerId 仅作操作标识及同名设备的确认规则。
- [x] 3.5 为 MCP 显示名优先级、空组织 fallback 和同名候选输出增加测试。

## 4. 验证与交付

- [ ] 4.1 运行 `pnpm exec tsc --noEmit`、相关 Vitest 测试、`cargo fmt --all`、`cargo clippy --workspace -- -D warnings` 和 workspace cargo check。（已运行；workspace Clippy 被既有 core lint 阻断）
- [x] 4.2 运行 `openspec validate add-device-aliases-and-groups --strict`。
- [ ] 4.3 手动验证：两台同名设备、一个设备多分组、删除分组、取消配对清理，以及 MCP 发送前澄清目标。
