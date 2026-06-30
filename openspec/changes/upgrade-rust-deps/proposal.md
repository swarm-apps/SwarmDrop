## Why

SwarmDrop 的几个被版本号"卡住"的 Rust 依赖落后于上游最新版：rmcp 锁在 1.x（最新 2.0.0，含 streamable-HTTP session leak 安全修复 #934）、keyring 锁在 3.x（最新 4.x）、sha2/chacha20poly1305 停留在 RustCrypto 0.10 旧波。本次借 RustCrypto 协调发布波一次性把这些主版本依赖拉齐到当前最新可用版，避免 generic-array/hybrid-array 与旧 rmcp 长期并存、并吸收上游安全修复。

经审计，宽松约束的依赖（tauri/axum/tokio/uuid/chrono 等）已由 cargo update 自动跟进到最新，sea-orm 已是最新 rc.41（2.0 正式版未发），specta 三件套锁死在唯一最新 rc——这些**不在本次范围**。本次仅处理需要改 Cargo.toml 约束（及少量代码）才能升的 4 个主版本依赖。

## What Changes

分两批落地，第一批可纯靠 `cargo test` 验证，第二批需真机验证后单独合入。

**第一批（编译期可验证，低风险）**

- **sha2 0.10.9 → 0.11**：workspace 约束升级；`dht_key.rs` 源码零改动（SHA256 摘要值不变，DHT key 兼容旧节点）。顺带删除 `src-tauri` 中未被引用的 sha2 死依赖。
- **chacha20poly1305 0.10.1 → 0.11**：`crates/core` 约束升级；重写 `transfer/crypto.rs::generate_key()`（`aead::OsRng` 不再 re-export，改用 `Generate` trait + getrandom），可选把 deprecated 的 `XNonce::from_slice` 换成 `From`。外部调用点零改动。
- **rmcp 1.8.0 → 2.0.0**：`src-tauri` 约束升级；`mcp/tools.rs` 把 `Content::text` 改 `ContentBlock::text`，`mcp/resources.rs` 把 `RawResource/no_annotation` 改为直建 `Resource`。features 不变，axum 0.8 兼容。**BREAKING**（依赖大版本跨越）：v2 给 streamable-HTTP 新增 Host/Origin 白名单（默认含 127.0.0.1），需冒烟验证本地 MCP 连通性。

**第二批（需真机验证，谨慎）**

- **keyring 3.6.3 → 4.1.2**：把 `src-tauri/Cargo.toml` 三个 per-target keyring 块合并为单行 `keyring = "4.1.2"`（v4 移除了 apple-native/windows-native/linux-native-sync-persistent/crypto-rust/vendored 旧 feature，默认 v1 facade 按平台自动选后端）。`host/keychain.rs` 源码零改动（6 个方法 + NoEntry + Display 全兼容）。**BREAKING**（潜在用户数据影响）：v4 三平台后端实现均更换（Linux dbus→zbus），老用户升级后可能读不到旧身份条目 → PeerId 重置 → 需重新配对（架构已容忍"找不到即重建"，但属一次性破坏，需 release note 提示并真机验证）。

## Capabilities

### New Capabilities
- `rust-dependency-currency`: 把安全/身份关键依赖维持在受支持的最新版，并约束此类升级必须保持可观测行为不变（SHA256 摘要值、加解密语义、MCP 契约、keychain 读写语义），keyring 等仅 release 生效的后端升级必须经签名 release 包逐平台真机验证后方可合入。

### Modified Capabilities
<!-- 本次升级本身不改变任何现有能力的行为：SHA256 摘要值不变、XChaCha20Poly1305 加解密语义不变、MCP server 工具/资源对外行为不变、keychain 读写 API 不变。故对现有 spec 无 delta。keyring 的"老条目可能读不到 → 身份重建"已被现有 keychain-based-identity 架构容忍，不构成对其需求的修改。 -->

## Impact

- **依赖（Cargo.toml）**：
  - `Cargo.toml`（workspace）：`sha2` 0.10.9 → 0.11
  - `crates/core/Cargo.toml`：`chacha20poly1305` 0.10.1 → 0.11
  - `src-tauri/Cargo.toml`：删除死依赖 `sha2`；`rmcp` 1.1.0 → 2；三个 per-target `keyring` 块合并为 `keyring = "4.1.2"`
- **代码**：
  - `crates/core/src/transfer/crypto.rs`（generate_key 重写 + 可选 nonce From）
  - `src-tauri/src/mcp/tools.rs`、`src-tauri/src/mcp/resources.rs`（rmcp 2.0 类型改名）
  - `crates/core/src/pairing/dht_key.rs`、`src-tauri/src/host/keychain.rs`：源码不动（仅随依赖升级验证）
- **传递依赖**：引入 hybrid-array 0.2 / aead 0.6 / rand_core 0.10（RustCrypto 波）、keyring-core 1.x + 各平台 store crate；rmcp 单一直接依赖方，无版本冲突。
- **CI / 验证**：`cargo build/clippy/test` 覆盖第一批；keyring 需正式签名 release 包在 macOS/Windows/Linux 各做身份读写 + 重启 PeerId 稳定性 smoke。Linux runner 的 dbus/openssl apt 依赖在 v4（纯 Rust zbus）下可移除（保留无害）。
- **双仓同步**：`crates/core` 与 SwarmDrop-RN 共享，chacha20poly1305/sha2 升级后 RN 端 mobile-core 复用同一 core，需在 RN 仓同步验证一次。
