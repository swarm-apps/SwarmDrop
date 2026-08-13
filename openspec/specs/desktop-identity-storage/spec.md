# desktop-identity-storage Specification

## Purpose
TBD - created by archiving change desktop-identity-file-store. Update Purpose after archive.
## Requirements
### Requirement: 桌面身份存储不依赖系统安全存储

桌面端（macOS / Windows / Linux）SHALL 将设备身份与已配对设备列表存储为应用数据目录下的
文件，MUST NOT 依赖系统钥匙串 / 凭据管理器 / Secret Service。

存储行为 MUST NOT 随构建配置（debug / release）而改变——两种构建使用同一个后端。

启动过程 MUST NOT 因读取身份而产生任何用户交互（授权框、解锁提示）。

#### Scenario: 冷启动零交互

- **WHEN** 应用启动并读取设备身份与已配对设备列表
- **THEN** 不出现任何系统授权或解锁提示，读取直接成功

#### Scenario: debug 与 release 行为一致

- **WHEN** 分别以 debug 与 release 构建启动应用
- **THEN** 两者从同一位置、以同一格式读写身份，不存在编译期分叉的存储后端

#### Scenario: 未签名或签名变更不影响读取

- **WHEN** 应用二进制的代码签名标识发生变化（重新构建、版本更新）
- **THEN** 身份读取不受影响，不出现授权提示

### Requirement: 密钥材料与业务数据分文件存储

设备密钥材料（Ed25519 keypair、WebRTC Direct 证书 PEM）SHALL 存储在独立文件中，
与已配对设备列表分开。已配对设备列表的写入 MUST NOT 导致密钥文件被重写。

在 unix 平台上，密钥文件 SHALL 设置为仅所有者可读写（0600）。

密钥文件 SHALL 存放在本机数据目录（非漫游目录），使其不随域漫游配置文件同步。

#### Scenario: 更新已配对设备不触碰密钥文件

- **WHEN** 已配对设备列表因配对、解除配对或对端改名而更新
- **THEN** 仅设备列表文件被写入，密钥文件的内容与修改时间不变

#### Scenario: unix 上密钥文件权限受限

- **WHEN** 密钥文件被创建或更新（在 Linux / macOS 上）
- **THEN** 该文件权限为 0600

#### Scenario: 密钥文件不落在漫游目录

- **WHEN** 在 Windows 上解析密钥文件路径
- **THEN** 路径位于本机数据目录（`%LOCALAPPDATA%`），而非漫游目录（`%APPDATA%`）

### Requirement: 身份文件写入是原子的

密钥文件的写入 SHALL 是原子替换：先写入同目录的临时文件并落盘，再重命名覆盖目标。
任何时刻中断（断电、进程被杀）后，该文件 MUST 要么是上一个完整版本，
要么是新的完整版本，MUST NOT 处于截断或部分写入状态。

#### Scenario: 写入中断后文件仍可用

- **WHEN** 密钥文件写入过程中进程被中断
- **THEN** 目标文件仍是中断前的完整内容，应用下次启动能正常读取该身份

#### Scenario: 写入完成后内容完整

- **WHEN** 密钥文件写入正常完成
- **THEN** 目标文件包含新的完整内容，且临时文件不残留

### Requirement: 身份读取失败时不静默生成新身份

当身份文件存在但无法解析时，系统 MUST NOT 将其当作「无身份」而静默生成新身份——
那会让一次可恢复的读取故障表现为「所有已配对设备无故消失」。

#### Scenario: 身份文件损坏时报错而非重置

- **WHEN** 密钥文件存在但内容无法解析为有效身份
- **THEN** 身份初始化返回错误并向用户呈现原因，不生成新身份、不覆盖原文件

#### Scenario: 身份文件不存在时正常生成

- **WHEN** 密钥文件不存在（首次启动）
- **THEN** 生成新身份并写入

### Requirement: 对外表述与实际存储位置一致

面向用户与开发者的表述（文档站、README、应用内节点状态诊断、项目架构文档）SHALL 准确
描述身份的实际存储形态，MUST NOT 声称身份存放于系统钥匙串。

应用内诊断信息 SHALL 使用户能够定位到身份的实际存放位置。

#### Scenario: 节点状态诊断显示实际位置

- **WHEN** 用户在节点状态弹窗中展开诊断信息
- **THEN** 「身份存放位置」一项描述的是实际的文件存储，而非系统钥匙串

#### Scenario: 公开文档不宣称使用系统钥匙串

- **WHEN** 审视文档站首页与 README 中关于身份存储的表述
- **THEN** 其描述与桌面端的实际实现一致

### Requirement: 身份存储位置不可经环境变量在生产构建中重定向

fixture 用的数据目录覆盖机制 SHALL 仅在 debug 构建中生效。release 构建 MUST 始终使用
平台默认目录，MUST NOT 允许环境变量改变身份文件的读写位置。

#### Scenario: release 忽略数据目录覆盖

- **WHEN** release 构建在设置了数据目录覆盖环境变量的情况下启动
- **THEN** 身份仍从平台默认目录读写，环境变量被忽略

