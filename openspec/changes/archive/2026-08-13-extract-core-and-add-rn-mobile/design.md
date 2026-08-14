## Context

SwarmDrop 当前的核心能力已经不再只是 Tauri 命令层的小后端。`src-tauri/src` 同时承载了 P2P 网络、设备管理、配对、传输协议、文件读写、数据库、MCP、Android 文件访问和 Tauri IPC。这个结构让桌面端开发很直接，但移动端迁移到 React Native 时会遇到两个问题：

- RN 端无法复用 Tauri IPC、Tauri `AppHandle`、Tauri Channel、Tauri 插件和 `tauri-plugin-android-fs`。
- 如果直接在 RN/JS 里重写 P2P、加密、分块传输和配对逻辑，会复制项目最有价值也最容易出错的部分。

SwarmNote 已经验证了一条更稳的路线：把业务能力抽成平台无关 Rust core，Tauri 桌面端和 Expo RN 移动端分别作为 host。RN 端通过 `uniffi-bindgen-react-native` 生成 Turbo Module/JSI 桥接，移动 UI 使用 Expo Router、Zustand、Lingui、NativeWind 和 RN primitives。

本变更把 `keychain-based-identity` 吸收为 core 抽离中的身份存储边界：设备 identity 不应由前端 Stronghold 流程拥有，而应由 host 提供安全存储能力，core 只依赖 trait。

目标架构：

```text
                 +----------------------+
                 |    swarmdrop-core    |
                 | identity / network   |
                 | pairing / transfer   |
                 | device / persistence |
                 +----------+-----------+
                            |
                     host traits
                            |
        +-------------------+--------------------+
        |                                        |
+-------v--------+                       +-------v--------+
| Tauri desktop  |                       | Expo RN mobile |
| IPC / windows  |                       | UniFFI / JSI   |
| keyring        |                       | SecureStore    |
| Tauri fs       |                       | file picker/fs |
| updater/MCP    |                       | mobile UI      |
+----------------+                       +----------------+
```

## Goals / Non-Goals

**Goals:**

- 抽离 `crates/core`，让网络、配对、传输、设备、身份、数据库核心逻辑脱离 Tauri。
- 保持桌面端现有功能可用，让 `src-tauri` 成为 host adapter。
- 新建 `../swarmdrop-mobile`，采用 SwarmNote Mobile 的 Expo RN + UniFFI 架构。
- RN MVP 跑通身份初始化、onboarding、配对、设备列表、网络状态、前台发送和接收。
- 将密码/Stronghold 问题改写成 host secret storage 边界，默认路线对齐系统 keychain/secure store。
- 让后续 iOS/Android 文件访问差异集中在 host file access adapter 中。

**Non-Goals:**

- 不在第一阶段追求 RN UI 与桌面端 100% 功能等价。
- 不实现后台长期传输、系统分享扩展、App Store/Google Play 发布。
- 不重写 `swarm-p2p-core`。
- 不把 MCP Server 作为 RN MVP 的目标能力。
- 不在 core 中直接依赖 Tauri、Expo、React Native、UniFFI 或平台插件。

## Decisions

### Decision 1: 使用 SwarmNote 风格的 Rust workspace

将仓库根 workspace 调整为包含：

- `crates/core`
- `crates/entity`
- `crates/migration`
- `src-tauri`

`libs/` 继续作为独立 submodule/workspace，通过 path dependency 引用。

备选方案：
- 保持 `entity` 和 `migration` 在 `src-tauri` 下。这样短期移动少，但 core 依赖数据库模型时会反向依赖 Tauri 目录。
- 新建独立仓库承载 core。这样发布清晰，但本地联调和大规模迁移更重。

理由：SwarmNote 已验证该布局适合“桌面 host + RN host”共享 Rust core。

### Decision 2: `swarmdrop-core` 不依赖 Tauri

`crates/core` SHALL 不引入 `tauri`、Tauri plugin、`tauri::AppHandle`、`tauri::ipc::Channel`。核心事件通过 `EventBus` trait 发出；需要进度流的长任务通过 core 事件或 host callback trait 表达。

备选方案：
- 保留 Tauri 类型并在 RN wrapper 中模拟。这样会把 Tauri 的 host 模型泄漏到移动端。

理由：core 必须能被 Tauri、RN、测试 harness 甚至未来 CLI 使用。

### Decision 3: Host trait 先抽最硬的边界

第一批 host traits：

- `KeychainProvider`: 设备 identity keypair 的持久化。
- `EventBus`: 设备、网络、配对、传输事件分发。
- `AppPaths`: app data、cache、download 或临时目录。
- `FileAccess`: 文件 source/sink、目录枚举、分块读写、保存位置。
- `Notifier`: 系统通知或空实现。
- `UpdateInstaller`: 桌面 updater/Android APK 安装等 host 能力，core 不直接拥有。

备选方案：
- 一次性抽象所有平台能力。风险是过度设计。
- 只抽 identity 和 event，文件访问稍后处理。风险是传输 MVP 无法在 RN 端验证。

理由：文件传输产品的 MVP 必须包含文件读写，因此文件访问要尽早成为边界。

### Decision 4: 先迁桌面到 core，再建 RN bridge

顺序：

1. 创建 core 并迁移纯业务模块。
2. Tauri desktop host 适配 core，确保现有桌面行为不倒退。
3. 创建 RN 项目和 UniFFI wrapper。
4. 暴露 RN MVP 所需 API。

备选方案：
- 先新建 RN 项目。UI 会很快出现，但没有共享 core，最终会卡在桥接和重复逻辑。
- 先只做 keychain/passwordless。它是对的，但会在 core 抽离时再次移动。

理由：桌面端是现有功能基准，先让桌面通过 core 跑起来，RN 才有稳定目标。

### Decision 5: RN 项目独立 sibling 仓库/目录

新建 `../swarmdrop-mobile`，参考 `../swarmnote-mobile`：

- Expo SDK / Expo Router。
- RN new architecture compatible。
- NativeWind + RN primitives。
- Zustand + Lingui。
- `packages/swarmdrop-core` 承载 UniFFI Turbo Module。

备选方案：
- 放在 SwarmDrop monorepo 内。共享路径简单，但移动端 native build、node_modules、Expo prebuild 会显著增加主仓复杂度。
- 继续 Tauri mobile。避免新项目，但生态和原生文件访问体验都受限。

理由：SwarmNote Mobile 已经采用 sibling 项目模式，迁移经验和工具链可复用。

### Decision 6: RN bridge 使用 wrapper crate，不给 core 加 UniFFI 注解

`../swarmdrop-mobile/packages/swarmdrop-core/rust/mobile-core` 依赖 `swarmdrop-core`，定义 UniFFI 友好的 records/enums/objects/callback traits。共享 core 不直接使用 `#[uniffi::export]`。

备选方案：
- 直接在 `swarmdrop-core` 上加 UniFFI 导出。短期少写 wrapper，但会污染 core API。

理由：SwarmNote Mobile 已证明 wrapper 层可以隔离 UniFFI 约束和生成代码。

### Decision 7: RN MVP 只承诺前台文件传输

第一版移动端只保证 app 前台运行时的 P2P 节点、配对和发送/接收。后台接收、系统分享入口、长时间后台传输放到后续变更。

备选方案：
- 一开始就做后台传输。iOS/Android 生命周期差异大，会拖慢主线验证。

理由：先证明 shared core + RN bridge + 文件访问 + P2P 互通，再扩大移动能力。

## Risks / Trade-offs

- [抽 core 范围过大] -> 按“桌面仍能编译运行”的里程碑切片迁移，不一次性重写所有模块。
- [Tauri 类型隐藏耦合多] -> 先用 `rg "tauri::|AppHandle|Channel|Emitter"` 建迁移清单，每个残留都转为 host trait 或 desktop adapter。
- [文件访问抽象设计不稳] -> MVP 先支持 RN DocumentPicker 复制到 cache 和 app 私有目录接收，SAF/公共目录作为后续增强。
- [UniFFI 类型限制] -> wrapper crate 做类型投影，core 保持 idiomatic Rust。
- [桌面回归风险] -> 每个 core 迁移阶段都跑 `cargo test`、`cargo clippy` 和 `pnpm build`，并手测配对/发送/接收。
- [keychain 迁移和 RN 主线互相扩大范围] -> 身份边界先抽 trait；legacy Stronghold 数据按破坏性变更废弃，不进入 RN 主线。
- [两个 OpenSpec 变更重叠] -> `keychain-based-identity` 视为被本变更吸收，实施时优先执行本变更。

## Migration Plan

1. 建立 workspace 和 `crates/core` 骨架。
2. 迁移无 host 依赖或依赖较少的模块：error、protocol、device、database ops。
3. 抽 `EventBus`、`KeychainProvider`、`AppPaths`、`FileAccess`，再迁移 network/pairing/transfer。
4. 重建 Tauri desktop host：commands 变成 core thin wrappers，事件从 core 转发到 Tauri frontend。
5. 完成桌面验证，确保现有流程仍能跑通。
6. 新建 `../swarmdrop-mobile` 和 `packages/swarmdrop-core` UniFFI wrapper。
7. 暴露 RN MVP API 并实现移动端 onboarding、配对、设备、发送/接收界面。
8. 用桌面端和 RN 端做跨端配对与文件传输验证。

Rollback strategy：在 core 迁移早期保留 Tauri host 的行为基线；每个模块迁移后单独提交/标记任务。RN 项目作为 sibling 新项目，不影响桌面发布路径。

## Open Questions

- RN 项目是否沿用 `com.yexiyue.swarmdrop` 的移动 package/bundle id，还是使用新的 dev id 到稳定后再切换？
- Android RN MVP 接收文件是否先保存到 app 私有目录，还是第一版就要求公共 Download 目录？
- `keychain-based-identity` 是直接归档为 superseded，还是保留作为本变更的参考 change？
- `MCP` 是否完全留在 Tauri desktop host，还是 core 保留可复用查询能力但 host 决定是否启用 server？
