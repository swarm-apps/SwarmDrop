## 1. 依赖和范围对齐

- [x] 1.1 确认 `drop-inbox-and-transfer-activity` 已落地，或明确需要对接的 Inbox API/save behavior。
- [x] 1.2 确认自动接收使用收件箱行为，而不是直接写入任意最终目录。
- [x] 1.3 修改共享设备类型前，复查当前配对和已配对设备持久化路径。

## 2. 共享策略类型

- [x] 2.1 新增 `DeviceTrustLevel` 枚举，包含 `owned`、`collaborator`、`temporary` 和 `blocked`。
- [x] 2.2 新增 `DeviceReceivePolicy` record，包含自动接收、确认、大小、目录、relay、保存行为、MCP-send 和过期字段。
- [x] 2.3 扩展 `PairedDeviceInfo`，增加带默认值的 trust 和 receive-policy 字段。
- [x] 2.4 为每个 trust level 增加默认策略模板构造函数。
- [x] 2.5 按需更新 specta/uniffi 暴露类型。

## 3. 持久化和迁移

- [x] 3.1 让不含 policy 字段的旧已配对设备记录反序列化为 `collaborator` trust 和需要确认的策略。
- [ ] 3.2 为 keychain/file-keychain 增加加载旧 paired-device JSON 的测试。
- [x] 3.3 确保设备元数据更新不会重置已有策略字段。
- [x] 3.4 增加读取和更新已配对设备 trust level / policy 的 core API。

## 4. 策略评估

- [x] 4.1 实现策略评估器，返回 `AutoAccept`、`RequireConfirmation` 或 `Reject { reason }`。
- [x] 4.2 对入站 transfer offer 执行 blocked-device 拒绝。
- [x] 4.3 执行 temporary 过期和接收限制。
- [x] 4.4 自动接收前执行最大传输大小和目录允许规则。
- [x] 4.5 执行 relay 路径自动接收 gate。
- [x] 4.6 当策略无法解析时使用需要确认的安全 fallback。

## 5. 入站 Offer 集成

- [x] 5.1 在发出普通入站 offer 确认事件前插入策略评估。
- [x] 5.2 当 owned 设备策略允许时自动接受 offer。
- [x] 5.3 将自动接收流程接入收件箱 receive/save behavior。
- [x] 5.4 为活动与恢复记录或事件增加策略决策上下文。
- [x] 5.5 确保自动接收的失败/中断会话留在活动与恢复中，且只有完成后才创建收件箱条目。

## 6. Tauri Commands 和 Events

- [x] 6.1 新增带 trust/policy 字段的已配对设备列表 typed command。
- [x] 6.2 新增更新 trust level 和 receive policy 的 typed commands。
- [x] 6.3 为策略自动接收/拒绝结果增加 event payload 或 projection 字段。
- [x] 6.4 重新生成 TypeScript bindings 并验证新增共享类型。

## 7. 桌面端 UI

- [x] 7.1 为已配对设备卡片/详情增加信任徽标。
- [ ] 7.2 增加配对后分类 UI，用于选择自有设备或协作者设备。
- [x] 7.3 在已配对设备操作菜单/详情中增加设备策略编辑 UI。
- [ ] 7.4 增加阻止/解除阻止操作，并提供清晰确认文案。
- [x] 7.5 更新入站 offer UI 文案，覆盖策略 fallback、拒绝和自动接收场景。

## 8. 验证

- [x] 8.1 增加策略默认值和策略评估器决策的单元测试。
- [x] 8.2 增加旧已配对设备迁移、设备元数据更新保留策略的测试。
- [ ] 8.3 增加自动接收、需要确认、策略拒绝的入站 offer 集成测试。
- [x] 8.4 运行 `openspec validate trusted-device-policies`。
- [x] 8.5 运行相关 Rust 测试、前端 typecheck 和 bindings 生成检查。
- [ ] 8.6 手动验证：自有设备自动接收入收件箱，协作者设备需要确认，阻止设备被拒绝。
