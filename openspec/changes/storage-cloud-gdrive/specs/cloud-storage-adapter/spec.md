# Spec Delta

## Purpose

宿主侧云目的地 adapter 的共同边界。它向接收编排提供发布已校验暂存文件、恢复
发布及返回远端对象身份的能力；首期 Google Drive 使用原生 Drive API，后续
阿里云盘与 OSS 可采用各自满足契约的实现。

## ADDED Requirements

### Requirement: 端口按云发布用例定义

`CloudPublisher` SHALL 接受账户 ID、接收记录键、目标相对路径、已校验的暂存
文件与根哈希，并返回带 provider、账户 ID、远端对象 ID、显示路径的对象引用。
provider 私有的 upload ID、session URI、分片编号和 HTTP 参数 MUST NOT 出现在
公共端口、前端 IPC 或共享 `crates/core` / `crates/transfer` 的数据类型中。

#### Scenario: 发布得到远端对象引用

- **WHEN** 云上传成功完成
- **THEN** 调用方得到可持久化的远端对象引用，不需要了解 Drive 文件创建协议

### Requirement: 凭证只通过账户管理器领取

存储 adapter SHALL 按操作从 `cloud-account-auth` 领取带有效期的 access token。
它 MUST NOT 读取 refresh token / client_secret，也 MUST NOT 在 OpenDAL 或其他 SDK
中启用隐式刷新。401 后最多经账户管理器刷新并重试一次；仍失败则呈「需要重新
连接」，不得在 adapter 内无限重试。

#### Scenario: 阿里云盘轮换由账户管理器持久化

- **WHEN** 未来的阿里云盘 adapter 需要新的 access token
- **THEN** 刷新与轮换只发生在账户管理器，adapter 不持有轮换后的 refresh token

### Requirement: provider 必须证明发布所需能力

每个 provider adapter SHALL 明确证明其元数据写入与查询、远端对象身份、
有界上传、跨进程恢复或失效后本地重传云段的能力。OpenDAL 可以是某个 adapter
的内部实现，但仅 `write=true` 或「可分片」不足以证明完整契约。缺失某项能力时
SHALL 使用受限且如实声明的实现，或阻止该 provider 出现在可选目的地列表中。

#### Scenario: GDrive 的 OpenDAL 写入不满足断点续传

- **WHEN** OpenDAL GDrive backend 仍只有一次性写入且没有应用可持久化的上传会话
- **THEN** GDrive adapter 使用原生 Drive API 实现上传，不把该 backend 标为可恢复发布

### Requirement: 上传检查点按 provider 私有格式持久化

需要跨进程恢复的 adapter SHALL 持久化其私有检查点，并把本地暂存身份与远端会话
绑定。含 bearer 能力的 session URI / 签名 URL SHALL 以 0600、同目录临时文件与
原子替换保存，MUST NOT 出现在日志、诊断导出或 IPC。恢复时以 provider 的远端
状态为准；会话过期只重启云段，不使已校验的 P2P 暂存失效。

#### Scenario: 完成响应丢失

- **WHEN** provider 已完成对象创建，但本地没有写入完成结果
- **THEN** adapter 先查询接收记录键确认远端对象，再决定是否重新上传

### Requirement: 错误归一且保留恢复动作

adapter SHALL 把凭证失效、暂时不可达或限速、目的地配置错误、冲突和操作不受
支持归一为有限类别。错误文本与日志 MUST NOT 含 access token、refresh token、
session URI 或签名 URL；错误 SHALL 保留账户、provider、操作与可否重试的信息。

#### Scenario: 限速可重试

- **WHEN** provider 返回限速响应
- **THEN** 上层收到可重试类别和退避建议，不收到原始含敏感 URL 的错误串

### Requirement: adapter 与 wasm 编译面隔离

`crates/storage-cloud` 及其厂商 HTTP / OpenDAL 依赖 MUST NOT 进入 wasm 目标的
依赖图；共享的目的地和对象位置类型 SHALL 可编译到 wasm。若未来某个 adapter
使用 OpenDAL，其版本 SHALL 在 workspace 统一声明并记录具体能力探针结果。

#### Scenario: 桌面新增云后端后浏览器构建仍通过

- **WHEN** 运行 `./scripts/check-wasm.sh`
- **THEN** 所有 wasm 面 crate 编译通过，云存储实现不在依赖图中
