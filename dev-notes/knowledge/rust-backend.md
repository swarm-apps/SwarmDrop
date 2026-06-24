# Rust Backend

## 概览

Rust 端的项目特有约束：crates/core 与 src-tauri 边界、specta IPC 类型映射、SeaORM/SQLite、libp2p P2P。常规 Rust 风格查 `/rust-best-practices`，async 模式查 `/rust-async-patterns`，Tauri IPC 查 `/tauri-v2`，SeaORM 查 `/sea-orm-2`。

## 模块边界

### 业务逻辑放 crates/core，src-tauri 是薄壳

`src-tauri/src/lib.rs` 用 `pub use swarmdrop_core::pairing;` 等 alias 把 core 模块路径桥进 crate（所以代码里 `crate::pairing::*` / `crate::protocol::*` 仍然有效）。**桌面壳唯一保留业务逻辑的模块是 `transfer/`**，其它都已迁移到 core。

**正确做法**：
- 加新业务逻辑/类型默认放 `crates/core`，让 SwarmDrop-RN 也能复用
- 桌面特定（keychain / 文件系统路径 / Tauri command 包装）才放 src-tauri
- 改 core 时跑 `cargo check -p swarmdrop-core --features specta`，再跑 `cargo check -p swarmdrop` 确认桌面壳不破

**相关文件**：`crates/core/src/lib.rs`、`src-tauri/src/lib.rs`、`dev-notes/architecture/core-desktop-mobile-boundaries.md`

## IPC 类型 (specta)

### bindings.ts 是自动生成的

`src-tauri/src/setup.rs:104` 在 debug build 时调用 `specta.export(..., "../src/lib/bindings.ts")`，每次 `pnpm tauri dev` 启动都会重写。

**不要做**：
- 手动改 `src/lib/bindings.ts`——会被下次 dev 启动覆盖
- 把 bindings.ts 当成"前端可改的契约"

**正确做法**：
- 改 IPC 类型 → 改 Rust 端 struct → 运行一次 `pnpm tauri dev`（或 `cargo run`，会在 setup hook 里触发导出）→ bindings.ts 自动更新
- 临时手改 bindings.ts 只用于"先让 tsc 通过、稍后再启 dev 重新导出"

### specta 需要开 chrono feature

`Cargo.toml` 里 `specta` 必须含 `chrono` feature，否则 `chrono::DateTime<Utc>` 无法 `derive(specta::Type)`。SwarmDrop 已配置（见 `crates/core/Cargo.toml` + `src-tauri/Cargo.toml`）。

### 跨 IPC 的时间类型用 DateTime<Utc>

specta + chrono 会把 `DateTime<Utc>` 映射成 ISO 8601 字符串（前端 `string` 类型）。前端 `new Date(isoString)` 自动正确解析。

**不要做**：
- 用 `i64` 当 IPC 时间戳——前端容易把秒当毫秒（`new Date(秒数)` 解析成 1970 年附近），导致 timer 死循环（见配对码每秒重生 bug 的修复 commit `8d298e5`）

**例外**：DHT 跨设备记录（`ShareCodeRecord`）保持 `i64` Unix 秒以稳定线路格式 + 节省 record 体积。From 转换里手写 `.timestamp()`。

**相关文件**：`crates/core/src/pairing/code.rs`

## Clippy / dead_code

### 用 #[expect(...)] 替代 #[allow(...)]

项目里清一色用 `#[expect(clippy::xxx, reason = "...")]` 而非 `#[allow]`。Rust 1.81+ 的语义是：标了 expect 的 lint 一旦"自然消失"会反向报警，避免遗留的过期 allow。

**正确做法**：
```rust
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub fn insert_session(...) { ... }
```

**相关文件**：`crates/core/src/database/ops.rs`、`crates/core/src/transfer/receive.rs`

## P2P / 异步

### 启动顺序：plugin → updater → database → start command

`src-tauri/src/setup.rs` 里 plugin 在 Builder::default() 注册；updater + database 在 setup() hook 里初始化并注入 Tauri state。**P2P 节点不在启动期自动起**——前端调 `commands::start()` 才创建 `NetClient` + `PairingManager`。

**相关文件**：`src-tauri/src/setup.rs`、`src-tauri/src/lib.rs` 的 `start` 命令

### 断点续传恢复必须双端发布 TransferResumed

恢复协议有两条入口：接收方发起 `ResumeRequest`，发送方发起 `ResumeOffer`。无论哪一端被动收到恢复请求，只要本端重建了 live session，都必须发布 `CoreEvent::TransferResumed`，让 host 把 paused 历史重新提升为 active session。

**正确做法**：
- `handle_resume_request_impl` 重建 `SendSession` 后发布 `TransferResumed { direction: Send }`
- `handle_resume_offer_impl` 重建 `ReceiveSession` 后发布 `TransferResumed { direction: Receive }`

**不要做**：
- 只标记 DB 为 `Transferring` 或只重建 core session；前端 store 不会自动从 history 推断 live session，另一端会停留在 paused 状态。

**相关文件**：`crates/core/src/transfer/resume.rs`、`src/stores/transfer-store.ts`

### 主动取消必须通知对端并写 cancelled

取消不是本地停止任务：本端要取消 live session、通知对端 `TransferRequest::Cancel`、写入 DB `Cancelled`，对端收到后也要标记 cancelled 并发出友好的 UI 提示。

**正确做法**：
- 发送方 `cancel_send` 也要像接收方一样发送 `Cancel`，不能只 `session.cancel()`
- 发送方 `waiting_accept` 还没有 `SendSession`，必须通过 `outbound_offers` 记录并在 Offer 异步返回后撤回，避免对端已接受后继续隐藏传输
- 取消状态写入放在 `crates/core`，Tauri / RN host 只做薄命令封装
- 前端收到 `TransferFailedEvent` 中的 `对方取消` 时按 info toast 展示，不按错误处理

**相关文件**：`crates/core/src/transfer/send.rs`、`crates/core/src/transfer/receive.rs`、`src/stores/transfer-store.ts`

## 身份存储 (keychain)

### dev 用文件后端、release 用系统 keychain（ad-hoc 签名导致 keychain 拒读）

`pnpm tauri dev` 编译的是 **ad-hoc 签名（linker-signed）二进制**——`codesign -dvvv target/debug/swarmdrop` 显示 `flags=0x20002(adhoc,linker-signed)`、`TeamIdentifier=not set`，且 `Identifier` 带内容 hash **每次 rebuild 都变**。macOS login keychain 对 ad-hoc 签名进程访问限制极严，所有 `keyring` 请求（**连查询一个不存在的条目**）都返回 `errSecInteractionNotAllowed`（"Platform secure storage failure: User interaction is not allowed."，不弹授权框直接硬拒）。

表现：设备身份起不来 → `initialize_identity` 抛错 → core `identity.rs` 的 `provider.load_identity().await?` 直接 `?` 传播（`keychain.rs` 只把 `NoEntry` 转 `Ok(None)`，其它错误一律 `Err`，连"生成新身份"退路都没有）→ 前端 `deviceId` 为 null → 点"启动节点"静默无反应。**删 keychain 条目无效**（是签名问题、非条目问题，新签名读旧条目/连查询都被拒）。

**正确做法**：
- 身份存储后端按 build 类型分叉，cfg 边界**唯一集中**在工厂 `crate::host::keychain_provider(&app)`：
  - `#[cfg(debug_assertions)]` → `FileKeychainProvider`（`app_data_dir/dev-identity.json` 明文持久，写后 `chmod 0600`）
  - `#[cfg(not(debug_assertions))]` → `DesktopKeychainProvider`（系统 keychain）
- 工厂返回 `Arc<dyn KeychainProvider>` 统一两分支静态类型（cfg 分支返回不同具体类型，`-> impl Trait` 无法统一）；core 函数签名是 `P: KeychainProvider + ?Sized`，用 `&*provider` 传入。
- 文件后端必须**持久**（keypair 存盘、复用），否则每次重启换 PeerId 破坏配对测试。`load_identity` 在文件缺失/keypair 空时返回 `Ok(None)`（绝不 `Err`），让 core 走"生成新身份并 save"路径。
- 调用 `Arc<dyn KeychainProvider>` 的 trait 方法**不需要** `KeychainProvider` 在 scope（trait object 走 vtable）；从具体 struct 换成 `Arc<dyn>` 后记得删掉原 `use ...::KeychainProvider`，否则 unused import warning。

**不要做**：
- 不要在 `DesktopKeychainProvider` 内部塞 `if-cfg` 降级——release 也可能在 keychain 偶发报错时误把明文私钥落盘；且降级逻辑散落每个方法。独立 provider + cfg 门控 `#[cfg(debug_assertions)] pub mod file_keychain;` 让 release 二进制根本不含文件后端代码。
- 给新增 `#[tauri::command]` 透传 `app: AppHandle` 改变了命令签名（如 `remove_paired_device` 补 app），但 Tauri 按类型注入、不占前端参数位，前端 invoke 不变；改后跑一次 `pnpm tauri dev` 重新导出 bindings 即可。

**相关文件**：`src-tauri/src/host/file_keychain.rs`、`src-tauri/src/host.rs`（`keychain_provider` 工厂）、`crates/core/src/identity.rs`、`src-tauri/src/host/keychain.rs`
