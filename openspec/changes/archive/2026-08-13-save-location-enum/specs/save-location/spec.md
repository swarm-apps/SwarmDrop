## ADDED Requirements

### Requirement: SaveLocation 枚举类型

系统 SHALL 使用 `SaveLocation` tagged union 类型表示接收方的保存位置，包含两个 variant：
- `Path`：桌面端文件系统绝对路径，携带 `path: string` 字段
- `AndroidPublicDir`：Android 公共目录，携带 `subdir: string` 字段（如 `"SwarmDrop"`）

该类型 SHALL 在 Rust 端（`entity` crate）和 TypeScript 前端同时定义，使用 `serde(tag = "type")` 进行序列化，JSON 格式为 `{"type":"path","path":"..."}` 或 `{"type":"androidPublicDir","subdir":"..."}`.

#### Scenario: 桌面端接收文件时创建 SaveLocation

- **WHEN** 桌面端用户接受文件传输并选择保存目录 `/home/user/Downloads/SwarmDrop`
- **THEN** 系统 SHALL 创建 `SaveLocation::Path { path: "/home/user/Downloads/SwarmDrop" }`

#### Scenario: Android 端接收文件时创建 SaveLocation

- **WHEN** Android 端用户接受文件传输
- **THEN** 系统 SHALL 创建 `SaveLocation::AndroidPublicDir { subdir: "SwarmDrop" }`

### Requirement: 数据库持久化 SaveLocation

数据库 `transfer_sessions.save_path` 列 SHALL 存储 JSON 序列化的 `SaveLocation`。列类型保持 `TEXT NULL` 不变。

#### Scenario: 保存桌面端传输记录

- **WHEN** 桌面端传输完成，保存路径为 `/home/user/Downloads/SwarmDrop`
- **THEN** 数据库 `save_path` 列 SHALL 存储 `{"type":"path","path":"/home/user/Downloads/SwarmDrop"}`

#### Scenario: 保存 Android 端传输记录

- **WHEN** Android 端传输完成
- **THEN** 数据库 `save_path` 列 SHALL 存储 `{"type":"androidPublicDir","subdir":"SwarmDrop"}`

#### Scenario: 读取保存位置

- **WHEN** 从数据库加载传输历史记录
- **THEN** `save_path` 列的 JSON 内容 SHALL 被反序列化为 `SaveLocation` 枚举

### Requirement: 旧数据 Migration

系统 SHALL 提供数据库 migration，将旧格式 `save_path` 字符串迁移为 JSON 格式的 `SaveLocation`。

#### Scenario: 迁移桌面端旧数据

- **WHEN** 旧数据 `save_path` 为绝对路径字符串（如 `/home/user/Downloads/SwarmDrop` 或 `C:\Users\...`）
- **THEN** migration SHALL 将其转换为 `{"type":"path","path":"/home/user/Downloads/SwarmDrop"}`

#### Scenario: 迁移 Android 端旧数据

- **WHEN** 旧数据 `save_path` 为 `"Download"`
- **THEN** migration SHALL 将其转换为 `{"type":"androidPublicDir","subdir":"SwarmDrop"}`

#### Scenario: NULL 值保持不变

- **WHEN** 旧数据 `save_path` 为 NULL
- **THEN** migration SHALL 保持 NULL 不变

### Requirement: TransferCompleteEvent 使用 SaveLocation

`TransferCompleteEvent` SHALL 使用 `save_location: Option<SaveLocation>` 替代原有的 `save_path`、`file_uris`、`save_dir_uri` 三个独立字段。

#### Scenario: 桌面端传输完成事件

- **WHEN** 桌面端传输完成
- **THEN** 事件 SHALL 携带 `saveLocation: { type: "path", path: "/actual/save/dir" }`，不再携带 `fileUris` 和 `saveDirUri`

#### Scenario: Android 端传输完成事件

- **WHEN** Android 端传输完成
- **THEN** 事件 SHALL 携带 `saveLocation: { type: "androidPublicDir", subdir: "SwarmDrop" }`，不再携带 `fileUris` 和 `saveDirUri`

### Requirement: acceptReceive 命令使用 SaveLocation

Tauri `accept_receive` 命令的保存位置参数 SHALL 从 `save_path: String` 改为 `save_location: SaveLocation`。

#### Scenario: 桌面端接受传输

- **WHEN** 桌面端用户接受传输，选择保存路径 `/home/user/Downloads/SwarmDrop`
- **THEN** 前端 SHALL 调用 `acceptReceive(sessionId, { type: "path", path: "/home/user/Downloads/SwarmDrop" })`

#### Scenario: Android 端接受传输

- **WHEN** Android 端用户接受传输
- **THEN** 前端 SHALL 调用 `acceptReceive(sessionId, { type: "androidPublicDir", subdir: "SwarmDrop" })`

### Requirement: 前端根据 SaveLocation 类型打开文件

`openTransferResult` 函数 SHALL 根据 `SaveLocation.type` 分支处理打开逻辑。

#### Scenario: 桌面端打开已完成传输的文件夹

- **WHEN** 用户点击"打开文件夹"，`saveLocation` 为 `{ type: "path", path: "/some/dir" }`
- **THEN** 系统 SHALL 使用 `revealItemInDir` 或 `openPath` 打开对应文件夹

#### Scenario: Android 端打开已完成传输的文件夹

- **WHEN** 用户点击"打开文件夹"，`saveLocation` 为 `{ type: "androidPublicDir", subdir: "SwarmDrop" }`
- **THEN** 系统 SHALL 动态 resolve `Download/SwarmDrop` 目录的 content URI，然后调用 `showViewDirDialog` 打开；若 resolve 失败，SHALL 回退到打开第一个已保存文件的 URI

#### Scenario: 从历史记录打开 Android 端文件夹

- **WHEN** 用户从传输历史记录点击"打开文件夹"，`saveLocation` 为 `{ type: "androidPublicDir", subdir: "SwarmDrop" }`
- **THEN** 系统 SHALL 同样动态 resolve URI 并打开，行为与活跃传输一致

### Requirement: completeSession 保留 SaveLocation

前端 `transfer-store` 的 `completeSession` 处理 SHALL 在移除活跃 session 前，将 `TransferCompleteEvent` 中的 `saveLocation` 更新到 session 中（供 UI 立即使用），同时该信息已通过数据库持久化可从历史记录恢复。

#### Scenario: 传输完成后立即点击打开

- **WHEN** 传输刚完成，session 尚未从内存移除
- **THEN** UI SHALL 能使用 session 中的 `saveLocation` 正确打开文件夹
