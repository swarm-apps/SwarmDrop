## ADDED Requirements

### Requirement: Host-agnostic core crate
系统 SHALL 提供 `swarmdrop-core` Rust crate，用于承载平台无关的设备身份、P2P 网络、设备管理、配对、文件传输、传输历史和核心事件逻辑。

#### Scenario: Core crate does not depend on Tauri
- **WHEN** 检查 `crates/core/Cargo.toml`
- **THEN** `swarmdrop-core` 不包含 `tauri` 或 Tauri plugin 依赖
- **AND** core 公开 API 不要求调用方传入 `tauri::AppHandle`、`tauri::Window` 或 `tauri::ipc::Channel`

### Requirement: Shared protocol types
系统 SHALL 将 P2P wire protocol 类型放入共享 core，使 Tauri desktop 和 RN mobile 使用同一套 request/response 定义。

#### Scenario: Desktop and mobile use same protocol
- **WHEN** desktop host 和 RN UniFFI wrapper 需要发送配对或传输请求
- **THEN** 它们通过 `swarmdrop-core` 暴露的协议类型或包装类型进行转换
- **AND** 不维护第二套 JS-only 或 host-only 协议定义

### Requirement: Core runtime entrypoint
系统 SHALL 提供 core runtime entrypoint，用于初始化身份、数据库、设备管理和可启动的网络生命周期。

#### Scenario: Host constructs core
- **WHEN** host 提供 keychain、event bus、app paths 和 file access adapters
- **THEN** core 可以构造应用级 runtime handle
- **AND** host 可以通过该 handle 查询设备身份、启动网络、停止网络和管理配对/传输

### Requirement: Core event model
系统 SHALL 通过 core 事件模型发布网络、设备、配对和传输状态变化。

#### Scenario: Device list changes
- **WHEN** core 处理网络事件导致设备列表变化
- **THEN** core 通过 `EventBus` 发布设备变化事件
- **AND** 不直接调用 Tauri emit 或 React Native callback 实现

### Requirement: Core testability
系统 SHALL 支持用内存 host adapters 测试核心身份、设备和传输状态逻辑。

#### Scenario: In-memory identity test
- **WHEN** 测试使用内存 keychain adapter 构造 core runtime
- **THEN** core 可以生成并复用稳定 PeerId
- **AND** 测试不需要启动 Tauri 或 React Native runtime
