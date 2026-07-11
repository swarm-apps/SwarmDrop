## ADDED Requirements

### Requirement: 本机设备别名
系统 SHALL 允许用户为已配对设备设置本机私有别名。别名 MUST 以 PeerId 关联，且 MUST NOT 改写对端名称、hostname、PeerId、信任策略、配对记录或 P2P 协议数据。

#### Scenario: 别名优先展示
- **WHEN** 已配对设备设置了非空本机别名
- **THEN** 设备中心、设备详情和发送页 SHALL 将该别名作为主显示名

#### Scenario: 清空别名
- **WHEN** 用户清空某设备的别名
- **THEN** 系统 SHALL 删除该设备的本机别名，并回退至对端名称、hostname 或短 PeerId

#### Scenario: 取消配对清理别名
- **WHEN** 用户取消与某 PeerId 的配对
- **THEN** 系统 MUST 删除该 PeerId 的本机别名和所有分组成员关系

### Requirement: 自定义设备分组
系统 SHALL 允许用户创建、重命名、排序和删除本机设备分组。一个已配对设备 MAY 属于零个或多个分组。

#### Scenario: 将设备加入多个分组
- **WHEN** 用户将同一已配对设备加入两个分组
- **THEN** 系统 SHALL 在两个分组中展示该设备，且两个条目引用相同的 PeerId

#### Scenario: 删除分组
- **WHEN** 用户删除一个分组
- **THEN** 系统 MUST 只删除该分组和成员关系，不得取消其中任何设备的配对、删除别名或改变信任策略

#### Scenario: 查看未分组设备
- **WHEN** 已配对设备不属于任何分组
- **THEN** 系统 SHALL 在「未分组」筛选项中提供该设备

### Requirement: 重名设备可辨识
系统 SHALL 为具有相同主显示名的设备提供次级身份信息，避免用户仅凭名称选择目标。

#### Scenario: 同名候选同时显示
- **WHEN** 可见设备列表中有两个或更多设备具有相同主显示名
- **THEN** 每个候选 SHALL 显示所属分组及 `hostname · 短 PeerId` 形式的次级身份信息

### Requirement: 组织数据仅本机持久化
本机组织数据（别名、分组、成员关系）SHALL 只保存在本机偏好中，MUST NOT 写入 keychain 的 `PairedDeviceInfo` 或同步给对端。

#### Scenario: 旧偏好或格式过旧
- **WHEN** 本机组织偏好不存在、字段缺失或格式过旧
- **THEN** 系统 SHALL 退化为空组织数据，并继续以对端名称、hostname 或短 PeerId 展示设备，不得因此阻断设备列表
