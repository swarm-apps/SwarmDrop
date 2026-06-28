# SwarmDrop-RN 架构说明

新人 / 跨仓贡献者来这个仓库前先读一遍。整体讲清楚 RN 端和共享 Rust core
之间怎么对话、为什么这么设计、生命周期和后台策略。

跟桌面端共享业务核心的细节见 [`SwarmDrop`](https://github.com/swarm-apps/SwarmDrop)
仓库的 `crates/core`。

---

## 一、双仓 + 三层结构

SwarmDrop 整体是双仓布局。**共享业务核心在桌面仓**，移动端通过 Cargo git `rev` 引用：

| 仓库 | 角色 | 跑在 |
|---|---|---|
| `swarm-apps/SwarmDrop` | 桌面 + 共享 core | macOS / Windows / Linux |
| `swarm-apps/SwarmDrop-RN`（本仓） | 移动 RN 壳 | iOS / Android |

```
SwarmDrop-RN (本仓)
└─ packages/swarmdrop-core/
   ├─ src/             ← uniffi-bindgen-rn 生成的 TS bindings
   └─ rust/mobile-core/ ← FFI wrap 层（uniffi）
                          ↓ git path
                       swarmdrop-core (跨平台业务核心，git rev 固定)
                          ↓
                       swarm-p2p-core (libp2p)
```

三层职责：

```
┌─────────────────────────────────────────────────────────────┐
│ React Native (TypeScript) — UI / Zustand store / Expo       │
│   src/app/, src/stores/, src/components/, src/core/         │
├─────────────────────────────────────────────────────────────┤
│ mobile-core (Rust, uniffi)                                  │
│   - MobileCore object (单例，握 Foreign trait + 业务句柄)    │
│   - 4 个 ForeignXxx trait（callback 给 RN 实现）             │
│   - 镜像 Record/Enum（不污染共享 core）                      │
├─────────────────────────────────────────────────────────────┤
│ swarmdrop-core (Rust, 桌面/移动共享, 不引入 uniffi/tauri)    │
│   - TransferManager / NetManager / PairingManager           │
│   - 用 trait（FileAccess/EventBus/KeychainProvider）          │
│     声明对宿主的依赖，实现由 host 注入                       │
└─────────────────────────────────────────────────────────────┘
```

**共享 core 的边界守得很严**：core 内部绝不出现 `uniffi` / `tauri` / `expo`，
也不出现平台特定的路径 / 通知 / keychain 实现 —— 它们都进入 host trait。

---

## 二、uniffi 桥接（mobile-core）

`packages/swarmdrop-core/rust/mobile-core/` 是把 swarmdrop-core 包成 RN
可调用接口的薄壳。一文带你看懂全部 12 个 .rs 文件。

### 2.1 入口对象 `MobileCore`

由 `app.rs` 定义，RN 启动时一次性 `new MobileCore(...)` 出来：

```ts
const core = new MobileCore(keychain, eventBus, fileAccess, dataDir);
```

`MobileCore` 内部握：

| 字段 | 类型 | 用途 |
|---|---|---|
| `keychain` | `MobileKeychainAdapter` | 适配成 core 的 KeychainProvider |
| `event_bus` | `MobileEventBusAdapter` | 适配成 core 的 EventBus |
| `file_access` | `MobileFileAccessAdapter` | 适配成 core 的 FileAccess |
| `data_dir` | String | SQLite 文件父目录（懒初始化 DB 用） |
| `keypair` | `Mutex<Option<Keypair>>` | 当前会话的身份密钥，懒载入 |
| `net_manager` | `Mutex<Option<NetManager>>` | startNode 后填充，shutdownNode 清空 |
| `db` | `Mutex<Option<Arc<DatabaseConnection>>>` | 首次需要时 `ensure_db()` 打开 |

业务方法分散在 `identity.rs` / `network.rs` / `device.rs` / `pairing.rs` /
`transfer.rs` 各模块的 `#[uniffi::export] impl MobileCore { ... }` 块里。
Rust 允许多 impl 块，ubrn proc-macro 全部能扫到，源码组织按业务领域分离。

### 2.2 四个 Foreign trait（RN 必须实现）

| Trait | 文件 | 由谁注入 | 适配为 core 的 |
|---|---|---|---|
| `ForeignKeychainProvider` | keychain.rs | RN 用 expo-secure-store | `KeychainProvider` |
| `ForeignEventBus` | events.rs | RN 用普通对象 + listeners | `EventBus` |
| `ForeignFileAccess` | file_access.rs | RN 用 expo-file-system | `FileAccess` |
| ~~`ForeignNotifier`~~ | — | 通知由 RN 直接走 `expo-notifications`，不进 core trait | — |

每个 ForeignXxx trait 都用 `#[uniffi::export(with_foreign)]` 声明，意味着
**Rust 调用、RN 实现**。callback 不能在 Rust 持锁时触发，否则
uniffi-bindgen-rn 会跨线程死锁 —— 这是反复踩过的坑。

### 2.3 镜像类型策略（Mirror Records）

core 的 `CoreEvent` / `HostFileMetadata` / `CoreSaveLocation` 等是平台无关
类型，**绝不**在共享 crate 上加 uniffi derive（会污染桌面端）。mobile-core
在自己的文件里建 `MobileXxx` 镜像，加 `From<CoreXxx>` 双向转换：

```rust
#[derive(uniffi::Record)]
pub struct MobileFileMetadata { ... }

impl From<HostFileMetadata> for MobileFileMetadata { ... }
impl From<MobileFileMetadata> for HostFileMetadata { ... }
```

新增字段时**两边都要补**，否则 RN 看到的形状会跟 core 期待的不一致。

### 2.4 错误传递 1:1 反映射

`error.rs` 定义 `FfiError`（uniffi 可序列化），`core::AppError` 通过
`From<AppError> for FfiError` 转过去；反过来 RN 抛错时 `FfiError` 经
`From<FfiError> for AppError` 转回去。**关键约束：保留错误源类型**：

- `FfiError::Database(msg)` → `AppError::Database(DbErr::Custom(msg))`
- `FfiError::Serialization(msg)` → `AppError::Io(io::Error::other("[host serde] ..."))`

这样核心层判断 `matches!(err, AppError::Database(_))` 仍然成立，桌面 / 移动
两端的错误处理代码可以共用。

---

## 三、生命周期

```
              ┌──────────────────────────────────────┐
              │  RN App 启动                          │
              │  src/app/_layout.tsx                  │
              └───────────┬──────────────────────────┘
                          │ Promise.all
                          ▼
       ┌────────────────────────┬───────────────────┐
       │ waitForOnboardingHydration │  initMobileCore │
       │   (Zustand persist)        │ (建 MobileCore  │
       │                             │  + 注入四个 Foreign)│
       └─────────────────────────────┴────────────────┘
                          │ ready=true
                          ▼
       SplashScreen.hideAsync + 渲染主导航
                          │ 用户进首页
                          ▼
              core.initializeIdentity()  ← src/stores/mobile-core-store.ts
                          │
                          ▼
              core.startNode([])         ← 内部 ensure_db() 懒开 SQLite
                          │
                  runtimeState=running
                          │
        ┌─────────────────┼─────────────────────┐
        ▼                 ▼                     ▼
   listDevices()    pushOffer/Progress    AppState lifecycle
                    （EventBus 推过来）
```

### 3.1 DB 懒初始化

`MobileCore::ensure_db()` 在第一次 `startNode` 时打开 SQLite，路径 =
`{data_dir}/swarmdrop.db`，data_dir 由 RN 端 `Paths.document.uri` 提供。
expo 的 uri 带 `file://` 前缀，`open_db` 内部 strip 掉。

之所以懒开而不是 constructor 时开：用户没进主流程（卡在 onboarding 或
unlock 时）就不该持有 DB 句柄，避免 onboarding 中途 kill 留下损坏的 db。

### 3.2 AppState 后台 / 前台

`src/app/_layout.tsx` 里挂了 `AppState` listener：

```
state=background/inactive:
   if runtimeState===running:
     wasRunningBeforeBackgroundRef = true
     core.shutdownNode()  // 关 NetManager 释放 socket / DHT 任务

state=active:
   if wasRunningBeforeBackgroundRef && runtimeState===stopped:
     wasRunningBeforeBackgroundRef = false
     core.startNode([])
```

为什么必须主动关：iOS / Android 在后台限制 socket、tokio runtime 跑着不会
自动停。如果不关，回前台后 NetManager 拿着已失效的连接 + 残留事件循环，
表现为「设备列表空白」「peer 探测不到」。

### 3.3 升级（仅 Android）

`src/stores/update-store.ts` —— UpgradeLink HTTP API + APK 整包升级。
启动 2s 后首次检查 + AppState 回前台时再检查（store 内部 12h 缓存兜底）。

iOS 走 TestFlight / App Store，整套机制 `Platform.OS !== 'android'` 短路。

---

## 四、事件流

Rust → JS 单向。core 触发 `CoreEvent` 通过 `EventBus.emit()` 投递到
`ForeignEventBus.emit(MobileCoreEvent)`，RN 的 `src/core/event-bus.ts`
分发到各 Zustand store + 选择性触发本地通知（fire-and-forget，因为
`emit` 来自 Rust 线程，不能 await）。

事件类型镜像在 `mobile-core/src/events.rs`，对应 RN 端的
`MobileCoreEvent` 联合类型由 ubrn 自动生成。

---

## 五、构建与发布

参见 [`native-build.md`](./native-build.md)。简短版：

```bash
pnpm install
pnpm prebuild                              # expo prebuild 出原生壳
pnpm --filter react-native-swarmdrop-core build:ios     # 或 build:android
pnpm ios   # 或 pnpm android
```

修改 mobile-core Rust 代码或 UniFFI 类型后必须重 build native artifacts（Android/iOS）并重跑
`pnpm --filter react-native-swarmdrop-core prepare`，否则 app 侧会继续读到旧的
`lib/typescript` package 类型。

---

## 六、何时改桌面 vs 何时改移动

| 改动类型 | 改哪里 |
|---|---|
| 业务逻辑 / 协议 / 加密 / DB schema | `swarmdrop/crates/core` |
| 新增 RN 屏 / 调整 UI | `SwarmDrop-RN/src/` |
| 新加 Foreign trait 方法 | mobile-core/*.rs **+** ubrn 重 build **+** RN 端实现 |
| 桌面专属（菜单 / 系统托盘） | `swarmdrop/src-tauri/` |
| 共享类型扩字段 | core 改完，mobile-core 镜像同步加字段，RN ts 跟着 ubrn 生成 |
