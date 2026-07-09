## ADDED Requirements

### Requirement: 注册为任意文件与文件夹的非默认「打开方式」处理器

桌面应用 SHALL 在三平台（macOS / Windows / Linux）把自己注册为任意文件与文件夹的「打开方式 / Open With」候选处理器，且 MUST NOT 成为任何文件类型的默认打开程序。

#### Scenario: 出现在「打开方式」但不抢默认

- **WHEN** 用户在文件管理器中右键任意一个文件查看「打开方式」列表
- **THEN** SwarmDrop 出现在候选列表中
- **AND** 该文件类型原有的默认打开程序保持不变

#### Scenario: 文件夹也可被打开

- **WHEN** 用户对一个文件夹使用「打开方式 → SwarmDrop」
- **THEN** 应用接收到该文件夹路径，与接收单个文件路径的行为一致

### Requirement: 在 macOS 捕获被打开的文件

macOS 上应用 SHALL 通过 `RunEvent::Opened` 接收被打开的文件 URL，无论应用是冷启动还是已在运行。

#### Scenario: 冷启动经「打开方式」进入

- **WHEN** 应用未运行，用户用 SwarmDrop 打开一个文件
- **THEN** 应用启动并在 `RunEvent::Opened` 中收到该文件的 URL
- **AND** 该路径不会因为「前端尚未就绪」而丢失

#### Scenario: 已运行时经「打开方式」进入

- **WHEN** 应用已在托盘运行，用户用 SwarmDrop 打开一个文件
- **THEN** 应用收到 `RunEvent::Opened` 携带的 URL 并前置主窗口

### Requirement: 在 Windows 与 Linux 捕获被打开的文件

Windows / Linux 上应用 SHALL 从命令行参数获取被打开的路径：冷启动读取 `std::env::args()`，已运行时读取 single-instance 回调传入的参数。

#### Scenario: 冷启动经命令行参数进入

- **WHEN** 应用未运行，操作系统以文件路径为参数启动它
- **THEN** 应用从启动参数中解析出存在的文件/文件夹路径

#### Scenario: 已运行时经 single-instance 参数进入

- **WHEN** 应用已在运行，用户再次用 SwarmDrop 打开一个文件触发第二个实例
- **THEN** single-instance 回调收到携带文件路径的参数并交由入口处理
- **AND** 主窗口被前置（保留原有唤出窗口行为）

### Requirement: 归一化路径并发布统一事件

应用 SHALL 把三种入口得到的目标归一化为本地绝对路径，并向前端发布单一的 `external-file-open` 事件（携带路径数组）。

#### Scenario: file:// URL 归一化为本地路径

- **WHEN** 入口收到形如 `file:///Users/x/a.pdf` 的 URL
- **THEN** 它被解码为本地绝对路径 `/Users/x/a.pdf` 后再发布

#### Scenario: 多文件与多实例合并为一次事件

- **WHEN** 用户一次打开多个文件，或操作系统为每个文件各拉起一个实例
- **THEN** 在一个短去抖窗口内到达的路径被合并进同一批次
- **AND** 前端只收到一个包含全部路径的 `external-file-open` 事件

### Requirement: 跨冷启动竞态可靠交付待处理的打开意图

当打开事件在前端订阅之前发生时，应用 SHALL 缓冲待处理路径，并提供让前端在挂载时主动拉取一次的命令；拉取后缓冲即清空。

#### Scenario: 事件早于前端就绪

- **WHEN** 冷启动的打开路径在前端订阅事件之前已产生
- **THEN** 前端在根处理器挂载时能主动取回这批待处理路径
- **AND** 取回后缓冲被清空，同一批路径不会被重复处理
