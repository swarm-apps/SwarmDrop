## 1. 本机组织数据与显示投影

- [x] 1.1 新增 `src/lib/device-organization.ts`：`DeviceOrganization` 类型、`organizedDeviceName`、`shortPeerId`、`deviceIdentityHint`、`deviceGroupNames`、`hasDuplicateOrganizedName`、`normalizeDeviceOrganization`。
- [x] 1.2 在 `preferences-store` 新增 `deviceOrganization` 状态与别名 / 分组创建 / 重命名 / 排序 / 删除 / 成员关系 / 清理操作，接入 `partialize` 与 `merge` 归一化。

## 2. 设备中心与详情体验

- [x] 2.1 设备中心（首页）增加「全部 / 未分组 / 用户分组」筛选与「管理分组」入口，保持在线优先排序，别名优先显示，同名显示次级身份信息。
- [x] 2.2 `DeviceCard` 支持传入显示名、分组名、身份提示与同名标记，渲染次级身份行。
- [x] 2.3 设备详情页增加别名与分组编辑 bottom sheet，取消配对时清理该 PeerId 组织数据。
- [x] 2.4 发送页目标设备使用本机显示名（别名优先），并传入 `startSend` 的 `peerName`。
- [x] 2.5 新增分组管理 bottom sheet（创建 / 重命名 / 上移下移 / 删除确认）。

## 3. core 同步与持久化刷新

- [x] 3.1 将 `rust/mobile-core/Cargo.toml` 四个 core git 依赖 bump 到含 `persist identified device names` 的 develop rev，并更新 sync-note。
- [x] 3.2 `MobileEventBusAdapter` 注入 keychain，在 `PairedDeviceAdded` 转发前 `upsert_paired_device` 持久化。
- [x] 3.3 `src/core/event-bus.ts` 新增 `PairedDeviceAdded` 分支刷新已配对缓存。
- [x] 3.4 重编原生库（Android arm64 已重编通过；iOS 同一 Rust 交叉编译），确认无 FFI 表面变化、`src/generated`+`cpp/generated` 零 diff、绑定无需改动。

## 4. 验证与交付

- [x] 4.1 `cargo check --manifest-path rust/mobile-core/Cargo.toml` 通过（core rev bump 编译验证，含全链 core@d62a4dd）。
- [x] 4.2 `pnpm typecheck` 与 `pnpm lint` 通过。
- [x] 4.3 `pnpm i18n:extract` 抽取新文案并补 en 翻译（en 0 missing）。
- [x] 4.4 `openspec validate add-mobile-device-aliases-and-groups --strict` 通过。
- [ ] 4.5 手测：两台同名设备、一台设备多分组、删除分组、取消配对清理、对端在线改名后离线仍保留新名。（待真机）
