## Context

已配对设备的 PeerId 是稳定的协议身份；`OsInfo.name` 和 hostname 是对端声明的身份信息。它们都不属于本机用户对“这是谁”的认知，因此不能被本机分类需求覆盖或回传给对端。当前桌面端将配对记录载入 `secret-store`，MCP 工具则从 `NetManager` 获取设备；两条展示路径都需要同一份本机组织数据。

## Goals / Non-Goals

**Goals:**

- 让用户为任意已配对设备设置本机私有别名。
- 支持将设备加入用户自定义的多个分组（例如“张三”“家庭”“工作设备”）。
- 在设备页、发送目标选择和 MCP 列表中用一致的显示名与分组消除重名歧义。
- 保持 PeerId 作为唯一操作标识，且不改变既有信任策略、配对和 P2P 协议。

**Non-Goals:**

- 不同步别名或分组到其他设备，不把它们写入 `PairedDeviceInfo`、keychain 或 Identify 协议。
- 不新增联系人账号、通讯录同步或跨设备共享分组。
- 不改变设备信任策略和自动接收策略的语义。

## Decisions

### 1. 组织数据为本机偏好，而非配对元数据

在 `preferences-store` 的持久化状态中新增版本兼容的 `deviceOrganization`：

```text
DeviceOrganization
  aliases: Record<PeerId, string>
  groups: Array<{ id: UUID, name: string, sortOrder: number }>
  groupDeviceIds: Record<GroupId, PeerId[]>
```

别名和分组都不是秘密，也不是启动 P2P 节点的前提，因此使用现有 `tauri-plugin-store` 比 keychain/`PairedDeviceInfo` 更合适。旧偏好文件缺少该字段时按空组织数据处理。

备选方案是将别名放入 `PairedDeviceInfo`。拒绝原因是它会混淆对端自报名称与本机标签，并使本地标签进入 keychain、core 和移动端共享模型。

### 2. 显示名优先级与歧义提示固定

展示文本按 `本机别名 → 对端 name → hostname → 短 PeerId` 解析。PeerId 不作为主名称；当同一可见列表中有多个相同显示名时，卡片和发送选择器必须额外显示分组路径及 `hostname · 短 PeerId` 的次级识别信息。

这保留了用户易读的主标签，同时让相同别名或相同设备名仍可被唯一确认。

### 3. 分组为多对多、无分组是有效状态

一个设备可属于零个或多个分组；一个分组可包含多个设备。设备页提供“全部”“未分组”和各用户分组的筛选入口，分组内仍按在线状态优先。删除分组仅删除其成员关系；取消配对时清理该 PeerId 的别名及全部分组成员关系。

备选方案是每个设备只能属于一个“联系人”。拒绝原因是设备常同时属于“张三”“家庭”“工作”等不同组织维度。

### 4. MCP 从同一持久化偏好读取组织投影

`list_paired_devices` 和 `list_available_devices` 在构造 MCP 输出时读取 `preferences.json` 中的组织数据，并返回：稳定 `peerId`、`displayName`、原始 `name`/`hostname`、`groups` 和 `identityHint`。若根据用户文本匹配出多个候选，MCP 使用显示名、分组和 identityHint 请求澄清，绝不以名称猜测后直接发送。

这避免新增 Tauri command 或把本机组织数据下沉到 core；MCP 的读取逻辑复用既有 preferences store 的双层 JSON 解析模式。

## Risks / Trade-offs

- **[Risk] 用户删除分组后误以为设备被删除** → 删除确认文案明确仅移除分类，且设备保留在“未分组”。
- **[Risk] 别名仍然重名** → 候选列表在同名时强制显示次级身份信息，MCP 必须要求确认。
- **[Risk] 用户取消配对后残留组织数据** → 取消配对路径统一清理该 PeerId 的别名和所有组成员关系。
- **[Risk] MCP 读取偏好失败或旧格式缺字段** → 退化为空组织投影，继续返回对端设备名称和 PeerId，不阻断发送前的确认流程。
- **[Trade-off] 本机组织数据不跨设备同步** → 这是保护隐私和避免与对端身份冲突的明确边界；跨设备同步留待后续账号/同步能力。

## Migration Plan

1. 为偏好 store 添加空的 `deviceOrganization` 默认值与解析 fallback。
2. 添加别名、分组和成员关系的 store 操作，并在取消配对时清理关联数据。
3. 统一设备展示投影，接入设备页和发送目标选择。
4. 为 MCP 设备投影读取组织数据并补充重名澄清规则。
5. 用旧偏好文件、重名设备、多分组、删除分组和取消配对场景验证；回滚时忽略 `deviceOrganization` 字段即可，不影响配对记录。

## Open Questions

- 分组管理首版是否放在设备页的独立“管理分组”弹窗，还是在设置页提供集中入口？本提案倾向设备页，减少导航切换。
- 发送页的筛选 UI 是否需要与设备页完全共享组件，还是先共享投影与逻辑、分别适配布局？
