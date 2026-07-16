## Why

移动端设备中心只用 `name → hostname` 展示已配对设备（`deviceDisplayName`）。当多台设备重名（比如两台 "iPhone"、两台叫 "MacBook-Pro" 的机器），或已配对设备变多时，用户无法从设备列表可靠地认出目标，容易把文件发错人。桌面端已通过 `add-device-aliases-and-groups` 落地本机别名 + 分组；移动端需要对齐同一套本机组织语义与体验。

同时，桌面端 core 修复了「对端改名后本机不持久化刷新」的问题（对端通过 libp2p Identify 广播新名称时，host 把刷新后的设备写回 keychain）。移动端引用的 core 版本落后于该修复，离线缓存里的对端设备名会一直停留在配对当时的旧值。

## What Changes

- 为已配对设备增加仅保存在本机的别名；别名优先于对端设备名与 hostname 展示，不改变 PeerId、对端设备名、信任级别或配对关系。
- 引入本机设备分组：用户可创建、重命名、排序和删除分组，并将一台设备加入零个或多个分组。
- 设备中心（首页）提供按分组筛选（全部 / 未分组 / 用户分组）与「管理分组」入口；未分组设备保持可见，在线优先排序不变。
- 设备详情页提供别名编辑与分组归属管理；取消配对时清理该 PeerId 的别名与全部分组成员关系。
- 重名设备在设备卡片与发送页展示分组与 `hostname · 短 PeerId` 次级身份信息以消歧。
- 同步 core 到含「Identify 刷新持久化设备名」的版本，并让 mobile-core 在收到 `PairedDeviceAdded`（Identify 刷新）时把设备写回 keychain，TS 侧刷新已配对缓存。

## Capabilities

### New Capabilities

- `device-organization`: 本机设备别名、设备分组及其持久化、清理与显示投影语义。
- `mobile-device-hub`: 移动端设备中心与详情页按本机组织浏览、别名优先展示、同名消歧与别名/分组管理入口。

## Impact

- `src/lib`: 新增 `device-organization.ts` 显示投影（别名 / 对端名 / hostname / 短 PeerId 优先级、同名判定、分组名、身份提示）。
- `src/stores/preferences-store.ts`: 新增可迁移的 `deviceOrganization` 偏好状态与别名 / 分组 / 成员关系操作，取消配对时清理。
- `src/app`: 设备中心（首页）、设备详情、发送页接入别名优先展示、分组筛选与别名 / 分组编辑。
- `packages/swarmdrop-core`: 将 mobile-core 的 core git 依赖升到含 `persist identified device names` 的 develop 版本；`MobileEventBusAdapter` 在 `PairedDeviceAdded` 时持久化，`event-bus.ts` 刷新已配对缓存。需重编原生库（无 FFI 表面变化，无需改动手写 TS 绑定）。
- 持久化：`deviceOrganization` 为本机偏好，不写入 keychain 的 `PairedDeviceInfo`，不同步到对端；旧用户默认无别名、无分组，不影响现有配对或发送行为。
