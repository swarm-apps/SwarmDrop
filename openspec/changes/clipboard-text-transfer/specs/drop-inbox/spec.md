## ADDED Requirements

### Requirement: 持久化并管理文本收件箱条目

系统 SHALL 为每次成功持久化的入站文本投递创建一个内容类型为 `text` 的 Drop Inbox 条目。该条目 SHALL 保留来源设备快照、接收时间、正文大小和可供详情读取的完整正文，且 SHALL 不要求关联文件传输会话或本地文件路径。

#### Scenario: 文本投递送达后创建条目

- **WHEN** 一次入站文本投递成功完成并持久化
- **THEN** 系统 SHALL 创建一个 `text` 收件箱条目
- **AND** 用户稍后在网络节点未启动时仍 SHALL 能列出、打开和复制该正文

#### Scenario: 文本投递未完成

- **WHEN** 一次入站文本投递被拒绝、取消、失败或尚待用户确认
- **THEN** 系统 SHALL NOT 创建文本收件箱条目

#### Scenario: 删除文本条目

- **WHEN** 用户删除一条文本收件箱条目
- **THEN** 系统 SHALL 删除或软删除该条目及其持久化正文
- **AND** 系统 SHALL NOT 请求删除本地文件或展示删除文件的确认文案
