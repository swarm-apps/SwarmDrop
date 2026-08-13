## Context

依赖审计（5 路并行实测 crates.io / docs.rs / GitHub changelog，非凭记忆）结论：SwarmDrop workspace 的 `Cargo.lock` 已较新，宽松约束依赖已自动跟进最新；真正被 requirement 卡住、需手动改的只有 4 个主版本依赖。它们使用面极小且高度集中：

| 依赖 | 现状→目标 | 唯一/主要使用点 |
|---|---|---|
| sha2 | 0.10.9 → 0.11 | `crates/core/src/pairing/dht_key.rs`（一发即用 `Sha256::digest`） |
| chacha20poly1305 | 0.10.1 → 0.11 | `crates/core/src/transfer/crypto.rs` |
| rmcp | 1.8.0 → 2.0.0 | `src-tauri/src/mcp/{tools,resources}.rs` |
| keyring | 3.6.3 → 4.1.2 | `src-tauri/src/host/keychain.rs` |

约束：`crates/core` 与 SwarmDrop-RN 双仓共享（chacha20poly1305/sha2 改动影响 RN）；workspace 已是 Rust 2024 edition；keyring 仅在 release build 生效（debug 走 `file_keychain`，且 macOS ad-hoc 签名进程被 Keychain 硬拒，见 `dev-notes/knowledge/rust-backend.md`）。

## Goals / Non-Goals

**Goals:**
- 把 4 个落后主版本依赖升至当前最新可用版，吸收 rmcp v2 的 streamable-HTTP session leak 安全修复（#934）。
- 保持所有对外可观测行为不变：SHA256 摘要值、XChaCha20Poly1305 加解密语义、MCP 工具/资源契约、keychain 读写语义。
- 用分批策略隔离风险：第一批编译期 + `cargo test` 可完全验证；第二批（keyring）的真机验证盲区单独承接。

**Non-Goals:**
- 不升 sea-orm（已是最新 rc.41，2.0 正式版未发）、不动 specta 三件套（锁死唯一最新 rc）、不上 tauri-plugin-biometry 0.3 rc。
- 不改 `libs/`（swarm-p2p）子模块的 libp2p（独立仓库、已最新 stable）。
- 不重构 crypto/MCP/keychain 的业务逻辑，仅做依赖适配性最小改动。

## Decisions

**D1：分两批，按"可否纯编译期验证"切分。** 第一批 sha2 + chacha20poly1305 + rmcp，全部能用 `cargo build/clippy/test` + 一次 MCP 冒烟坐实正确性；第二批 keyring 单独走，因其真实代码路径只在 release build 执行、dev/test 覆盖不到，必须出签名 release 包逐平台手测。两批拆成独立提交，第二批可在真机验证排期就绪后再合，不阻塞第一批。
- 备选：一次性全升 → 否决，会把 keyring 的"编译通过≠功能正确"风险捆绑进本可即时验证的第一批。

**D2：sha2 + chacha20poly1305 同批升级（RustCrypto 波）。** 两者同属 RustCrypto 2026-06 协调发布（aead 0.6 / digest 0.11 / hybrid-array 0.2 / rand_core 0.10）。同批升可让 hybrid-array 尽量统一，避免与旧 generic-array 长期并存（并存仅增编译体积、不冲突，但无意义）。
- 备选：只升 sha2（trivial）暂缓 chacha → 否决，留半波没有收益。

**D3：`generate_key()` 用 `Generate` trait 重写，而非保留 RNG 参数式 API。** chacha20poly1305 0.11 不再 re-export `aead::OsRng`，且 `KeyInit::generate_key(&mut rng)` 被 deprecated。改为 `Key::<XChaCha20Poly1305>::generate()`（getrandom 后端，无 rng 参数）。函数签名 `() -> [u8; 32]` 不变，5 个外部调用点（sender/receiver/resume）零改动。
- 备选：显式传入 `rand_core 0.10` 的 OsRng → 否决，额外引依赖且与上游推荐路径背离。

**D4：keyring 三个 per-target 块合并为单行 `keyring = "4.1.2"`。** v4 的默认 `v1` feature 即按 target 自动 set_default_store（macOS=apple-native-keyring-store、Windows=windows-native-keyring-store、*nix=zbus-secret-service-keyring-store）。旧的 apple-native/windows-native/linux-native-sync-persistent/crypto-rust/vendored feature 全被移除，保留会编译失败。
- 备选：保留 per-target 显式指定新 store crate → 否决，v1 facade 已封装平台分发，单行更简且与上游意图一致。

**D5：rmcp Host/Origin 白名单"先默认、按需放行"。** v2 给 streamable-HTTP 加了 DNS-rebinding 防护（`allowed_hosts` 默认含 127.0.0.1/localhost）。本项目本地绑定通常落在默认白名单内，不预先改配置；升级后冒烟连一次 MCP client，仅当被拒才在 `StreamableHttpServerConfig` 显式放行。
- 备选：升级即无脑放开全部 host → 否决，放弃了 v2 带来的安全收益。

## Risks / Trade-offs

- **[keyring 验证盲区]** 仅 release 生效 + macOS ad-hoc 拒读，`cargo test`/`pnpm tauri dev` 覆盖不到真实路径，编译通过≠功能正确 → **Mitigation**：第二批单独排期，强制出签名 release 包在 macOS/Windows/Linux 各做身份读写 + 连续重启 PeerId 稳定性 smoke；验证未过不合入。
- **[keyring 跨版本数据兼容]** v3→v4 三平台后端实现均更换，老用户旧条目可能读不到 → 触发身份重建 → PeerId 变化、需重新配对 → **Mitigation**：架构已容忍（`keychain-based-identity`："找不到即重建、非阻塞"）；在 release note 显式提示，真机验证时确认旧条目可读（若不可读至少不 panic、走重建）。
- **[rmcp 白名单连通性]** 客户端以非 localhost Host/浏览器 Origin 连接可能被新校验拒 → **Mitigation**：D5 的升级后冒烟；被拒即显式配置放行。
- **[RustCrypto 传递依赖多版本]** 若 stronghold 插件 / libs/core P2P 栈仍依赖旧 generic-array/rand_core，会出现同名 crate 多版本并存 → 仅增编译体积/二进制大小，不构成冲突，**接受**。
- **[MSRV / edition]** sha2/chacha20poly1305 0.11 要求 edition 2024 / MSRV 1.85 → 本机 rustc 1.95、workspace 已 edition 2024，满足；**接受**。
- **[双仓漂移]** chacha20poly1305/sha2 改 `crates/core` 后 RN 端 mobile-core 复用同一 core → **Mitigation**：本仓升级落地后，在 SwarmDrop-RN 平级 checkout 下跑一次 mobile-core 编译/构建确认不破。

## Migration Plan

1. **第一批**：改 3 处 Cargo.toml（sha2 workspace、chacha20 core、rmcp src-tauri）+ 删 src-tauri 死 sha2；改 crypto.rs（generate_key + 可选 nonce From）、tools.rs、resources.rs。`cargo update -p sha2 -p chacha20poly1305 -p rmcp` → `cargo build/clippy -D warnings/test` → MCP 冒烟。提交。
2. **RN 同步**：平级 checkout 下编译 SwarmDrop-RN mobile-core 确认 core 改动不破。
3. **第二批**：合并 keyring 为单行 4.1.2 → `cargo build --release`/clippy → 出签名 release 包逐平台真机 smoke（身份读写 + 重启 PeerId 稳定）→ 验证通过后提交 + release note 提示。
4. **回滚**：每批独立提交，任一批出问题 `git revert` 该提交并 `cargo update` 还原 Cargo.lock 即可；不涉及数据库 schema / 持久格式变更（DHT key 算法、加密格式均不变），无数据迁移回滚负担。

## Open Questions

- keyring v4 各平台 store 对旧条目的 service/account 属性映射是否与 v3 完全一致（决定老用户是否真会 PeerId 重置）——只能在第二批真机验证中坐实，无法静态确认。
