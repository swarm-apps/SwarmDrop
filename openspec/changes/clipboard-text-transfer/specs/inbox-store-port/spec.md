## ADDED Requirements

### Requirement: 文本收件箱载荷经共享端口持久化

系统 SHALL 通过 `InboxStore` 的共享领域语义创建和读取文本收件箱条目及其正文；SQL、移动共享 core 和浏览器持久化实现 SHALL 返回相同的文本、元数据与删除语义。调用方 SHALL NOT 直接访问某一端的数据库或浏览器存储以绕过该端口。

#### Scenario: 各端读取同一文本条目

- **WHEN** 桌面、移动或 Web 宿主读取一个已持久化的文本收件箱条目
- **THEN** 它们 SHALL 获得相同的正文、来源快照、接收时间、正文大小和内容类型
- **AND** 返回值 SHALL 不要求文件列表、文件路径或传输会话标识存在

#### Scenario: 文本载荷存储失败

- **WHEN** 文本条目或正文无法被完整持久化
- **THEN** 端口 SHALL 返回明确失败而非部分成功
- **AND** 调用方 SHALL NOT 将该投递报告为已送达
