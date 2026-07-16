## Context

移动端已配对设备有两条展示数据源：节点运行时 `list_devices`（NetManager 实时，含对端 Identify 最新名称）与 `list_paired_devices`（keychain 持久化骨架，离线兜底，进 `pairedDevicesCache` 并落 AsyncStorage）。两条路径在 `mergePairedDevicesWithCache` 合并后由 `deviceDisplayName` 统一渲染。本机组织（别名 / 分组）是纯本机认知，不属于对端身份，因此不能写回对端或 keychain 的 `PairedDeviceInfo`。桌面端已用同一决策落地（组织数据放 preferences store）。

移动端无单元测试基础设施（无 vitest / jest），因此本变更不引入单元测试，验证走 `tsc --noEmit` + `cargo check` + 原生构建 + 真机手测。

## Goals / Non-Goals

**Goals:**

- 让用户为任意已配对设备设置本机私有别名。
- 支持将设备加入用户自定义的多个分组。
- 在设备中心、设备详情和发送页用一致的显示名与分组消除重名歧义。
- 保持 PeerId 作为唯一操作标识，不改变既有信任策略、配对与 P2P 协议。
- 同步 core 修复，让对端改名在本机离线缓存里也能持久化刷新。

**Non-Goals:**

- 不同步别名或分组到其他设备，不写入 `PairedDeviceInfo`、keychain 或 Identify 协议。
- 不新增联系人账号、通讯录或跨设备共享分组。
- 不改变设备信任策略与自动接收策略语义。
- 移动端无 MCP，不涉及桌面端的 MCP 组织投影。

## Decisions

### 1. 组织数据为本机偏好，与桌面端同构

在 `preferences-store` 持久化状态中新增版本兼容的 `deviceOrganization`：

```text
DeviceOrganization
  aliases: Record<PeerId, string>
  groups: Array<{ id: UUID, name: string, sortOrder: number }>
  groupDeviceIds: Record<GroupId, PeerId[]>
```

字段、语义与桌面 `src/lib/device-organization.ts` 一致，便于跨端心智对齐。移动端偏好用 AsyncStorage + 手写 `merge`（无 `migrate`/`version`），故在 `merge` 里用 `normalizeDeviceOrganization` 对旧数据做清洗兜底（丢弃非法分组、剔除悬空成员关系与空别名）。分组 id 用 `expo-crypto` 的 `randomUUID()`（RN 无全局 `crypto.randomUUID`）。

### 2. 显示名优先级与消歧固定

展示文本按 `本机别名 → 对端 name → hostname → 短 PeerId` 解析。PeerId 不作主名称；当同一可见列表中出现相同主显示名时，设备卡片与发送页额外显示分组路径及 `hostname · 短 PeerId` 次级识别信息。

### 3. 分组为多对多、无分组是有效状态

一台设备可属于零个或多个分组。设备中心提供「全部」「未分组」和各用户分组筛选，组内仍按在线优先。删除分组只删成员关系；取消配对时清理该 PeerId 的别名与全部分组成员关系。

### 4. 管理入口贴合移动端形态

移动端无右键 / 下拉菜单：per-device 别名与分组编辑放在**设备详情页**（点设备卡进入详情，是移动端天然的「设备菜单」等价物），以 bottom sheet 承载别名输入 + 分组勾选 + 内联新建分组。分组的全局管理（重命名 / 排序 / 删除）与筛选放在**设备中心首页**——筛选 chips + 「管理分组」bottom sheet。

### 5. core 同步与 mobile-core 持久化

将 `rust/mobile-core/Cargo.toml` 的四个 core git 依赖（`swarmdrop-core`/`entity`/`migration`/`swarm-p2p-core`）从 `e6323d1` 升到 `d62a4dd`（develop tip，含 `persist identified device names`）。该区间对 `crates`/`libs` 仅有纯新增改动（`OsInfo: PartialEq` + `refresh_os_info`、event_loop 刷新函数、pairing manager 方法），mobile-core 现有 API 用法不受影响。

共享 core 的事件循环在 Identify 刷新已配对设备时会 publish `CoreEvent::PairedDeviceAdded`，但持久化是 host 职责（桌面在 `event_bus.rs` 里 `upsert_paired_device`）。移动端镜像该行为：给 `MobileEventBusAdapter` 注入 keychain，在 `publish` 转发前对 `PairedDeviceAdded` 调 `upsert_paired_device` 写回 keychain；TS `event-bus.ts` 新增 `PairedDeviceAdded` 分支触发 `loadPairedDevicesCache()`。此改动不新增 FFI 导出类型 / 方法（`MobilePairedDevice` 与该事件已存在），故手写 TS 绑定无需改动，只需重编原生库。

## Risks / Trade-offs

- **[Risk] 用户删除分组误以为设备被删除** → 删除确认文案明确仅移除分类，设备保留在「未分组」。
- **[Risk] 别名仍重名** → 同名时强制显示次级身份信息。
- **[Risk] 取消配对后残留组织数据** → 取消配对路径统一清理该 PeerId 的别名与全部分组成员关系。
- **[Risk] 旧偏好缺字段或格式过旧** → `normalizeDeviceOrganization` 退化为空组织，不阻断设备展示。
- **[Risk] core git rev bump 引入非预期 core 改动** → 已核对 `e6323d1..d62a4dd` 对 core crate 仅 3 文件纯新增，其余为 docs/chore/style/前端 refactor。
- **[Known limitation] keychain 读改写非原子** → 新增的 `PairedDeviceAdded` 持久化钩子与既有的 `remove_paired_device` / `update_paired_device_policy` / 配对 upsert 一样，对 keychain 的已配对清单做 load-modify-save，无共享锁。极端并发下（如取消配对与 Identify 刷新几乎同时）可能出现 lost-update。这是既有模式而非本变更引入；且取消配对后共享 core 的内存 `paired_devices` 已移除该 peer，`refresh_paired_device_os_info` 对已移除 peer 返回 None 不再 publish，故窗口极窄。彻底串行化 keychain 写入属独立架构改进，留待后续。
- **[Trade-off] 组织数据不跨设备同步** → 明确的隐私与身份边界，跨设备同步留待后续账号能力。

## Migration Plan

1. 新增 `deviceOrganization` 空默认值与 `merge` 归一化兜底。
2. 添加别名、分组、成员关系操作，取消配对时清理。
3. 统一设备显示投影，接入设备中心、设备详情与发送页。
4. bump core git rev，接入 mobile-core `PairedDeviceAdded` 持久化与 TS 缓存刷新，重编原生库。
5. 用旧偏好、重名设备、多分组、删除分组、取消配对清理与对端改名刷新场景手测；回滚只需忽略 `deviceOrganization` 字段并还原 git rev。

## Open Questions

- 分组排序移动端首版用「上移 / 下移」按钮（对齐桌面），是否后续换拖拽由体验反馈再定。
