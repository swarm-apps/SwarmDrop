## 1. 第一批 · sha2 0.11

- [x] 1.1 `Cargo.toml`（workspace.dependencies）：`sha2 = "0.10.9"` → `sha2 = "0.11"`
- [x] 1.2 删除 `src-tauri/Cargo.toml` 中未被引用的死依赖 `sha2`（src-tauri 代码零引用，已 grep 确认）
- [x] 1.3 确认 `crates/core/src/pairing/dht_key.rs` 无需改动（`Sha256::digest(...).to_vec()` 在 0.11 原样编译）
- [x] 1.4 Cargo.lock 已含 sha2 0.11.0（swarmdrop-core 直接依赖解析到 0.11.0；旧 0.10.9 仅 libp2p 传递依赖保留）

## 2. 第一批 · chacha20poly1305 0.11

- [x] 2.1 `crates/core/Cargo.toml`：`chacha20poly1305 = "0.10.1"` → `chacha20poly1305 = "0.11"`（features 不变，默认含 alloc + getrandom）
- [x] 2.2 重写 `crates/core/src/transfer/crypto.rs::generate_key()`：删除 `use chacha20poly1305::aead::OsRng;`，改用 `Key::<XChaCha20Poly1305>::generate()`（`Generate` trait，getrandom 后端），签名 `() -> [u8; 32]` 不变
- [x] 2.3 `crypto.rs` 中 `XNonce::from_slice(&nonce)` → `&XNonce::from(nonce)`（实测非可选：deprecation 警告会被 `-D warnings` 变硬错误）
- [x] 2.4 已确认 `new(key.into())`、import、以及 sender/receiver/resume 外部调用点零改动（编译 + 全测试通过）
- [x] 2.5 Cargo.lock 已含 chacha20poly1305 0.11.0（swarmdrop-core 直接依赖；旧 0.10.1 仅传递依赖保留）

## 3. 第一批 · rmcp 2.0

- [x] 3.1 `src-tauri/Cargo.toml`：`rmcp` 版本 `"1.1.0"` → `"2"`（features `server` / `transport-streamable-http-server` 不变）
- [x] 3.2 `src-tauri/src/mcp/tools.rs`：`rmcp::model::Content::text(...)` → `ContentBlock::text(...)`（两处）
- [x] 3.3 `src-tauri/src/mcp/resources.rs`：import 去掉 `AnnotateAble`/`RawResource`、加入 `Resource`；`RawResource::new(...).…​.no_annotation()` 改为 `Resource::new(...).with_description(..).with_mime_type(..)`
- [x] 3.4 已确认 `mcp.rs`、`mcp/server.rs` 零改动（编译通过，宏/trait/transport/builder 兼容）
- [x] 3.5 Cargo.lock 已含 rmcp 2.0.0（swarmdrop 唯一直接依赖，无多版本）

## 4. 第一批 · 编译与验证

- [x] 4.1 `cargo build --workspace` 通过（1m28s，仅 1 个既有 database.rs 警告）
- [x] 4.2 clippy：本次改动引入 **0 新警告**（stash 前后均 8 个，全为 develop 既有、不在改动文件内）。注：`-D warnings` 字面门槛因 8 个既有警告无法过，与本升级无关（见报告，待单独清理）
- [x] 4.3 `cargo test -p swarmdrop-core` 通过：单元 78/78 + e2e_transfer 11/11（含单文件/多块多文件/断点续传，覆盖新 crypto 加解密落盘）
- [ ] 4.4 MCP 冒烟：需运行 app + 本地 MCP client 实测（运行时验证，待手动执行；rmcp 改动为纯类型改名，编译+API 不变，主要验证 v2 新增的 Host/Origin 白名单）
- [x] 4.5 `cargo update --dry-run` 仅少量无关传递补丁（base45/camino/multibase/open），无回退、无冲突
- [x] 4.6 第一批已作为独立 commit 提交到 develop（`b919682`）

## 5. 第一批 · 双仓同步验证

- [x] 5.1 已验证 `crates/core` 在 RN 配置（默认 features、无 specta、走 uniffi）下 `cargo check` 通过。注：RN mobile-core 当前 git rev 钉在旧 commit `eb183af`（发版模式，非本地 path），故无法在此直接编译到本地改动；真正的 RN 端验证须在第一批发布/提交后、RN re-pin 到新 commit 时进行
- [x] 5.2 RN 待办（记录）：第一批落地后，把 `SwarmDrop-RN/packages/swarmdrop-core/rust/mobile-core/Cargo.toml` 四个 git rev 从 `eb183af` re-pin 到新 commit，再跑一次 RN mobile-core 构建确认 chacha 0.11/sha2 0.11 复用无碍

## 6. 第二批 · keyring 4.1.2

- [x] 6.1 `src-tauri/Cargo.toml`：三个 per-target keyring 块已合并为 `[dependencies]` 单行 `keyring = "4.1.2"`（已去除所有旧 feature）
- [x] 6.2 已确认 `src-tauri/src/host/keychain.rs` 源码零改动（编译通过；`pub mod keychain` 无条件编译，debug check 即覆盖）
- [x] 6.3 已提交 `2a4704c`：`cargo update -p keyring` → 4.1.2；`cargo check --workspace`（debug）+ `cargo check --release --workspace`（release cfg 工厂分支）+ `clippy --all-targets -D warnings` 全通过；Cargo.lock 新增 keyring-core 1.0 + apple/windows/zbus store crate
- [x] 6.4 已核查 `release.yml`：Linux 仅装标准 Tauri 依赖（webkit2gtk/appindicator/rsvg/patchelf），无 keyring 专属 libdbus/openssl，无需清理

## 7. 第二批 · 三平台真机验证（合入前置门 · 本环境无法执行，待人工）

- [ ] 7.1 macOS 签名 release 包：身份存入→读出一致、连续重启 PeerId 稳定；确认旧条目可读或至少走非阻塞重建
- [ ] 7.2 Windows release 包：同上身份读写 + 重启 PeerId 稳定
- [ ] 7.3 Linux release 包：在带 Secret Service provider（gnome-keyring/kwallet）环境验证；无 provider 时确认返回 NoDefaultStore 不 panic
- [ ] 7.4 在 release note 提示"老用户升级后可能需重新配对"（若真机验证显示旧条目不可读）
- [ ] 7.5 三平台验证全部通过后，第二批作为独立提交合入

## 8. 收尾

- [x] 8.1 `cargo fmt --all` 已执行（含在 style 清理提交 `082b5e6`），`fmt --check` 归零
- [x] 8.2 已更新 `dev-notes/knowledge/rust-backend.md`：记录 RustCrypto 0.11 迁移（aead::OsRng 移除 → Generate trait）、rmcp 2.0 类型改名（Content→ContentBlock、RawResource→Resource）、keyring 4.x feature 重构与 release-only 验证盲区
- [x] 8.3 质量已由 `clippy --all-targets -D warnings` + `fmt --check` 归零保证；改动为机械式依赖适配，无需额外 /simplify
