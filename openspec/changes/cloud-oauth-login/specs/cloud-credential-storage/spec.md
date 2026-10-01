# Spec Delta

## Purpose

云账户凭证（client 凭证、refresh_token、access_token 及其元数据）在本机的持久化
纪律：权限、原子性、隔离与防泄露。与 `desktop-identity-storage` 管辖的设备身份
平行，是 issue #132「token 与私钥同级 0600 存储」要求的规格化。

## ADDED Requirements

### Requirement: 凭证文件仅所有者可读写

云账户凭证 SHALL 存放于应用本机数据目录（非漫游目录）下，unix 平台上文件权限
MUST 为 0600（仅所有者可读写）。存储行为 MUST NOT 随构建配置（debug / release）
改变。

#### Scenario: unix 上凭证文件权限受限

- **WHEN** 凭证文件被创建或更新（Linux / macOS）
- **THEN** 该文件权限为 0600

#### Scenario: 凭证不落在漫游目录

- **WHEN** 在 Windows 上解析凭证文件路径
- **THEN** 路径位于本机数据目录（`%LOCALAPPDATA%` 语义），而非漫游目录

### Requirement: 凭证写入是原子替换

凭证文件的每次写入（含刷新凭证轮换的覆写）SHALL 以「同目录临时文件落盘 → 原子
重命名覆盖」完成。任何时刻中断后，该文件 MUST 是上一个完整版本或新的完整版本，
MUST NOT 处于截断或部分写入状态。

#### Scenario: 轮换覆写被中断后文件仍完整

- **WHEN** 刷新凭证轮换覆写过程中进程被中断，之后应用重启
- **THEN** 凭证文件要么是完整旧版，要么是完整新版；若旧版已被 provider 作废，账户按 `cloud-account-auth` 的规则进入「需要重新连接」

### Requirement: 单账户单凭证文件

每个已连接账户的凭证 SHALL 存放在独立文件中，按 provider 与账户标识寻址。一个
账户的凭证更新 MUST NOT 导致其他账户的凭证文件被重写。

#### Scenario: 刷新一个账户不触碰其他账户凭证

- **WHEN** 账户 A 的 token 轮换落盘
- **THEN** 账户 B 的凭证文件内容与修改时间不变

### Requirement: 凭证材料不出现在任何观测面

凭证本体（client_secret、refresh_token、access_token）MUST NOT 出现在日志、诊断
导出、传输历史、错误消息或返回给前端的 IPC 数据中；诊断信息 SHALL 使用账户标识
与状态字段定位问题。凭证 MUST NOT 参与设备配对数据的漫游同步。

#### Scenario: 日志与错误信息不含凭证

- **WHEN** 刷新失败并产生错误日志与用户可见错误
- **THEN** 日志与错误文本只含账户标识、provider 与失败类别，无任何凭证字段值

#### Scenario: 配对漫游不携带凭证

- **WHEN** 审视设备配对同步的数据面
- **THEN** 云账户凭证不在其中，新配对设备不继承任何云凭证

### Requirement: 删除凭证即清除残留

账户断开或凭证删除时，对应的凭证文件与写入过程中产生的临时文件 SHALL 一并被
清除；删除失败 SHALL 上报错误而非静默留下凭证。

#### Scenario: 删除后无临时文件残留

- **WHEN** 账户断开完成
- **THEN** 凭证目录下该账户的凭证文件与同名临时文件均不存在
