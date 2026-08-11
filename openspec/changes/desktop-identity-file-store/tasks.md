## 1. 端口层瘦身（先做，它会牵动三端）

- [x] 1.1 从 `crates/host` 删除 `IdentityMigrationState` 类型与 `KeychainProvider` 上的
      `load_migration_state` / `save_migration_state`
- [x] 1.2 `crates/core/src/identity.rs`：移除 `save_migration_state(Completed)` 调用与相关测试
- [x] 1.3 移动端 `mobile/packages/swarmdrop-core/rust/mobile-core/src/keychain.rs`、
      Web 端对应实现：删除两个方法的实现体
- [x] 1.4 `cargo check --workspace --all-targets` 通过（含 uniffi 桥）

## 2. 桌面文件后端（生产化）

- [x] 2.1 将 `src-tauri/src/host/file_keychain.rs` 改名为 `host/identity_store.rs`，
      去掉模块声明上的 `#[cfg(debug_assertions)]`，模块文档重写（不再是 dev-only 的说明）
- [x] 2.2 拆成两个文件：`identity.json`（keypair + webrtc cert PEM）与
      `paired-devices.json`（设备列表）；两个 trait 各读写各自的文件
- [x] 2.3 路径改用 `paths::app_local_data_dir()`（D6：Windows 上不落漫游目录）
- [x] 2.4 实现原子写：同目录临时文件 → `sync_all()` → `rename` 覆盖；两个文件都用，
      判据对 `identity.json` 强制
- [x] 2.5 **改掉容错读**（D3b）：文件不存在 → `Ok(None)`；存在但解析失败 → `Err`，
      绝不返回默认值、绝不覆盖原文件
- [x] 2.6 保留 unix 的 `0600`；Windows 不做显式 ACL 操作
- [x] 2.7 单测：原子写中断后旧内容完好、解析失败返回 `Err` 不覆盖、设备列表写入不触碰
      密钥文件（比对 mtime 或内容）

## 3. 删除 keychain 后端

- [x] 3.1 删除 `src-tauri/src/host/keychain.rs`
- [x] 3.2 `src-tauri/src/host.rs`：`DesktopSecretStore` 的 cfg 二选一收敛为单一实现，
      两个端口工厂保持零 cfg
- [x] 3.3 `src-tauri/Cargo.toml` 移除 `keyring` 依赖，更新 `Cargo.lock`
- [x] 3.4 确认 Linux 打包依赖（`tauri.conf.json` 的 `linux.deb.depends`）无需保留
      secret-service 相关项

## 4. fixture 与 demo 工具链

- [x] 4.1 确认 `paths.rs` 的 `SWARMDROP_DATA_DIR` 覆盖仍为 `#[cfg(debug_assertions)]`
      门控（D6 风险项：release 不允许经环境变量重定向身份位置）
- [x] 4.2 `crates/core/examples/seed_demo_profile.rs`：按新的两文件布局与 schema 生成 fixture
- [x] 4.3 `e2e/desktop/test/specs/demo/lan-transfer.demo.ts:145` 与
      `e2e/desktop/demo-asset-plan.md` 中的 `dev-identity.json` 文件名与
      `SWARMDROP_DESKTOP_IDENTITY_FILE` 约定同步更新
- [ ] 4.4 跑一次 demo fixture 流程，确认录制脚本仍能加载 profile

## 5. 对外表述同步（与实现同 PR，不留后续）

- [x] 5.1 `docs/app/(home)/page.tsx:413` 首页宣传语改为准确描述（文件位置 + 权限 +
      保护边界），移动端仍是系统安全存储要写清楚
- [x] 5.2 `README.md` 三处（37 / 116 / 141 行）
- [x] 5.3 `src/components/network/node-status-sheet.tsx:251` 的「系统钥匙串」改为实际位置，
      并让用户能据此定位到文件
- [x] 5.4 `pnpm i18n:extract` 并补 `src/locales/{zh,zh-TW,en}/messages.po` 三份译文
- [x] 5.5 `docs/app/app/_components/node-panel.tsx:268` 的三端对比说明
- [x] 5.6 `CLAUDE.md`：Tech Stack 表的 Security 行、Stronghold 段落下方关于 keychain 端口
      的说明、Key File Locations 表
- [x] 5.7 `DESIGN.md` 的 Node Status Contract 中「身份存放位置」文案判据
- [x] 5.8 全仓复查一遍：`grep -rn "钥匙串\|keychain\|keyring" src docs README.md DESIGN.md CLAUDE.md`
      确认无残留的错误表述（注释里描述移动端的除外）

## 6. 机器门禁

- [x] 6.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo test --workspace`
- [x] 6.2 `./scripts/check-wasm.sh --clippy`（端口层改动波及 wasm 七 crate，
      wasm job 的 clippy 是硬失败）
- [x] 6.3 `pnpm test` + `pnpm build`
- [x] 6.4 `docs/` 下 `pnpm typecheck` + `pnpm build`

## 7. 真机验证

- [x] 7.1 macOS 真机：首次启动生成 `identity.json`，**全程无授权框**。注：跑的是 debug
      构建——但本次改动的效果之一正是 debug 与 release **走同一条代码路径**（cfg 二选一
      已删），所以这条的说服力比改动前高得多；release 包仍建议发版前跑一次
- [x] 7.2 重启应用两次，`identity.json` 的 sha256 与 mtime **逐字节不变** → 是复用而非重新生成
- [ ] 7.3 配对一台设备 → 重启 → 设备列表仍在
- [x] 7.4 由单测 `corrupt_identity_file_errors_instead_of_resetting` 覆盖（写坏 → `Err`
      且原文件字节不变）。真机重复一次的边际价值低于把用户 profile 弄坏的风险
- [x] 7.5 真机确认 `-rw-------`（0600），位于 `~/Library/Application Support/com.yexiyue.swarmdrop/`（macOS 上 local 与 data 同目录）
- [ ] 7.6 Windows 与 Linux 各跑一次冷启动 + 配对（Rust CI 只跑 ubuntu，这两个平台的编译与
      运行问题不会在 PR 阶段暴露）

## 8. 发版与收尾

- [ ] 8.1 **（留给发版）** 发版说明写出 BREAKING：升级后需重新配对（现象是设备列表变空）。
      版本号三处同步也未做——本次只实施 change，发版是独立动作
- [ ] 8.5 **（留给用户决定）** 存量 keychain 条目清理：删掉 `keyring` 依赖后应用再也够不到
      那四条 `com.yexiyue.swarmdrop` 条目，其中 `device-identity` 是一把**已失去用途的
      Ed25519 私钥**，会无限期留在系统钥匙串里。macOS 清理：
      `security delete-generic-password -s com.yexiyue.swarmdrop -a device-identity`
      （另三条 `paired-devices` / `identity-migration-state` / `webrtc-direct-certificate` 同理）
- [x] 8.2 三道关：机器门禁全绿（含 `check-wasm.sh --clippy`）✅ ·
      `/simplify` 应用 11 条（其中一条是安全相关：`IdentityFile` 的 `#[derive(Debug)]`
      会绕过端口层给 `DeviceIdentityBytes` 手写的 redacting `Debug` 原样打印私钥）✅ ·
      `/code-review` 进行中
- [x] 8.4 `/simplify` 附带产出（超出原 tasks 范围，记录在此）：
      写入拆两级 `Durability`（设备列表不 fsync——macOS 的 `F_FULLFSYNC` 1–20ms 而它
      每次对端改名都写，「不损坏」靠的是 rename 不是 fsync）；`write_json_atomic` 收进
      一次 `spawn_blocking` 并改用 `mode(0o600)` 建临时文件（消掉「以默认 umask 权限
      承载私钥」的瞬间窗口）；`get_identity_file_path` 改走 `host::identity_file_path()`
      工厂；`ports.rs` 补 `KeychainProvider` 的**改名触发条件**（当没有任何一端还是
      keychain 时）——那条判据原先只在知识库里，直接翻 `crates/host` 的人拿不到
- [x] 8.3 已更新 `dev-notes/knowledge/rust-backend.md`：新增「把 dev 的权宜实现提为生产
      实现，必补两条：原子写、读取失败不降级」，并订正了端口分工表里「桌面 = 系统钥匙串」
      的过时描述 + 补一条「trait 名仍叫 KeychainProvider 但桌面实现不是 keychain」的说明
