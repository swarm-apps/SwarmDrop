## ADDED Requirements

### Requirement: 本机设备别名
系统 SHALL 允许用户为已配对设备设置本机私有别名。别名 MUST 以 PeerId 关联，且 MUST NOT 改写对端名称、hostname、PeerId、信任策略、配对记录或 P2P 协议数据。

#### Scenario: 别名优先展示
- **WHEN** 已配对设备设置了非空本机别名
- **THEN** 设备页、发送目标选择和 MCP 设备投影 SHALL 将该别名作为主显示名

#### Scenario: 清空别名
- **WHEN** 用户清空某设备的别名
- **THEN** 系统 SHALL 删除该设备的本机别名，并回退至对端名称、hostname 或短 PeerId

#### Scenario: 取消配对清理别名
- **WHEN** 用户取消与某 PeerId 的配对
- **THEN** 系统 MUST 删除该 PeerId 的本机别名和所有分组成员关系

### Requirement: 自定义设备分组
系统 SHALL 允许用户创建、重命名、排序和删除本机设备分组。一个已配对设备 MAY 属于零个或多个分组。

#### Scenario: 将设备加入多个分组
- **WHEN** 用户将同一已配对设备加入“张三”和“工作设备”两个分组
- **THEN** 系统 SHALL 在两个分组中展示该设备，且两个条目引用相同的 PeerId

#### Scenario: 删除分组
- **WHEN** 用户删除一个分组
- **THEN** 系统 MUST 只删除该分组和成员关系，不得取消其中任何设备的配对、删除别名或改变信任策略

#### Scenario: 查看未分组设备
- **WHEN** 已配对设备不属于任何分组
- **THEN** 系统 SHALL 在“未分组”筛选项中提供该设备

### Requirement: 重名设备可辨识
系统 SHALL 为具有相同主显示名的设备提供次级身份信息，避免用户仅凭名称选择目标。

#### Scenario: 同名候选同时显示
- **WHEN** 可见设备列表中有两个或更多设备具有相同主显示名
- **THEN** 每个候选 SHALL 显示所属分组及 `hostname · 短 PeerId` 形式的次级身份信息

#### Scenario: MCP 遇到歧义目标
- **WHEN** MCP 根据用户描述匹配到多个同名或同组候选设备
- **THEN** MCP MUST 使用显示名、分组和次级身份信息请求用户澄清，且 MUST NOT 直接发送文件

### Requirement: MCP 组织投影
MCP 的 `list_paired_devices` 与 `list_available_devices` SHALL 输出面向用户的设备组织投影，同时保留 PeerId 作为操作标识。

#### Scenario: 返回已组织设备
- **WHEN** MCP 查询一个已设置别名并加入分组的设备
- **THEN** 返回值 MUST 包含 `peerId`、`displayName`、原始 `name`/`hostname`、`groups` 和 `identityHint`

#### Scenario: 旧偏好或读取失败
- **WHEN** 本机组织偏好不存在、格式过旧或无法读取
- **THEN** MCP SHALL 退化为基于对端名称和 hostname 的设备投影，不得因此使设备查询失败
