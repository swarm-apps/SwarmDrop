## Why

macOS 上每次启动都要点授权框——而且点「始终允许」也不管用。

根因不是 keychain 本身，是 `src-tauri/tauri.conf.json` 的 `"macOS": { "signingIdentity": "-" }`
（ad-hoc 签名）。ad-hoc 签名没有稳定的 designated requirement（标识就是 cdhash，每次构建都变），
macOS 的 keychain ACL 按 DR 匹配调用方，于是**认不出是同一个应用**；也正因为没有稳定标识，
系统无法把它写进 item 的可信应用列表，「始终允许」形同虚设。

启动路径要读三条独立的 keychain item，每条各走一次 ACL 检查、各弹一次：

```
initialize_identity   → device-identity              ①
                      → paired-devices               ②
start                 → webrtc-direct-certificate    ③
```

这是 **macOS + release 构建**这一格独有的问题：debug build 走的已经是
`src-tauri/src/host/file_keychain.rs` 的明文文件后端；移动端走 iOS Keychain /
Android EncryptedSharedPreferences，沙箱内自己读自己的 item，不存在 ACL 提示。

出路只有两条：拿 Developer ID 签名（$99/年，根治且保留 keychain），或者放弃 keychain。
本变更选后者——这个项目的分发形态（自建 SwarmHive、明文 HTTP updater、自签 DMG）本来就没有
签名基础设施，keychain 在这里收不回它的代价。

## What Changes

- **`file_keychain.rs` 的文件后端从 dev-only 提为桌面唯一后端**，三个桌面平台统一
  （macOS / Windows / Linux）。`#[cfg(debug_assertions)]` 的编译期二选一整体删除。
- **拆成两个文件，而不是沿用 dev 那份单文件**：
  - `identity.json`（0600）——Ed25519 keypair + WebRTC Direct 证书 PEM。**近乎只读**：
    首次生成后几乎不再改写。
  - `paired-devices.json`——已配对设备列表。它是业务数据（`keychain.rs:95` 的注释自己
    写明「可导出、会整份覆写」），且会在 identify 观察到对端改名时被重写。
    与密钥同文件意味着高频写会不断擦写承载私钥的那个文件。
- **写入改为原子写**（临时文件 + `rename`）。dev 后端用的是直接覆盖的 `fs::write`——
  对 dev 无所谓，但**私钥文件写到一半断电等于身份不可恢复地丢失**，这是把 dev 代码提为
  生产代码必须补上的差异。
- **删除 `keyring` 依赖与 `src-tauri/src/host/keychain.rs`**。`keyring = "4.1.2"` 是无条件
  依赖，删除后 Linux 侧顺带甩掉 secret-service / D-Bus 那条运行时链路。
- **删除 `IdentityMigrationState` 端口**（`load_migration_state` / `save_migration_state`
  及其 `identity-migration-state` 存储位）。Stronghold 迁移逻辑清干净后，它在生产代码里
  **只写不读**——`load_or_create_identity` 只在首次生成身份时写一次 `Completed`，没有任何
  读者（仅测试还在读）。
- **BREAKING：不提供从 keychain 的迁移**。存量 keychain 中的身份不再被读取，应用会当作
  首次启动生成新身份 → **`device_id` 变化 → 所有已配对设备失效，需重新配对**。
  已确认当前 v0.16.x 发布版除作者本人外无真实用户，重新配对一次的代价低于写一次性迁移代码
  并长期携带它（与 Web 端「schema 直接换，不写迁移/回填/双写」同一个口径）。
- **同步全部对外表述**（散落 6 处，其中 2 处面向公众）。生物识别那次的教训是「改了实现
  没改宣传，文档站与 README 持续宣传一个不存在的功能」，本次表述与实现必须同 PR 合入。

## Capabilities

### New Capabilities
- `desktop-identity-storage`: 桌面端设备身份与已配对设备列表的持久化——存储形态、
  文件布局与权限、写入原子性、启动时零用户交互，以及对外表述与实现一致的约束。

### Modified Capabilities
（无。现有 specs 中没有描述身份存储位置的需求；节点状态弹窗里「身份存放位置」那一行的
文案判据住在 `DESIGN.md` 的 Node Status Contract，随本变更同步，见 Impact。）

## Impact

**Rust（桌面）**

| 文件 | 改动 |
|---|---|
| `src-tauri/src/host/file_keychain.rs` | 去 dev-only 门控；拆两个文件；原子写；重命名（模块名与文件名都不该再叫 `dev-` / `file_keychain`） |
| `src-tauri/src/host/keychain.rs` | 删除 |
| `src-tauri/src/host.rs` | `DesktopSecretStore` 的 cfg 二选一收敛为单一实现 |
| `src-tauri/Cargo.toml` | 移除 `keyring` |
| `crates/host/src/...` | 删除 `IdentityMigrationState` 及端口上的两个方法 |
| `crates/core/src/identity.rs` | 移除 `save_migration_state` 调用 |
| `crates/web/`、`mobile/.../keychain.rs` | 跟随端口收窄（各自的空实现随之删除） |

**移动端不改存储形态**：iOS Keychain / Android EncryptedSharedPreferences 无此问题。
副作用是三端身份存储形态出现分叉（桌面文件 / 移动系统安全存储 / Web IndexedDB），
文档需要明确写出这个分叉及其理由。

**测试与工具链**

- `crates/core/examples/seed_demo_profile.rs:120` 手工复刻了那份 JSON 的 schema，需同步。
- `e2e/desktop/test/specs/demo/lan-transfer.demo.ts:145` 与 `e2e/desktop/demo-asset-plan.md:139`
  硬编码了 `dev-identity.json` 文件名与 `SWARMDROP_DESKTOP_IDENTITY_FILE` 约定。
- `src-tauri/src/host/paths.rs` 的 `SWARMDROP_DATA_DIR` fixture 覆盖当前是 debug-only；
  身份文件成为生产存储后需重新确认该门控是否仍恰当。

**对外表述（6 处，须同 PR）**

| 位置 | 内容 |
|---|---|
| `docs/app/(home)/page.tsx:413` | 文档站首页宣传语「私钥交给系统钥匙串（macOS Keychain / Windows 凭据管理器 / Linux Secret Service）」 |
| `README.md`（3 处：37 / 116 / 141 行） | "held in the OS keychain" 等 |
| `src/components/network/node-status-sheet.tsx:251` | 节点状态弹窗诊断区「系统钥匙串」——用户可见 |
| `src/locales/{zh,zh-TW,en}/messages.po` | 上述文案的三份 catalog |
| `docs/app/app/_components/node-panel.tsx:268` | Web 端「另外两端的身份在系统钥匙串里」 |
| `CLAUDE.md` | 「私钥现由宿主 keychain 端口管理（桌面 = keyring 系统钥匙串）」及 Tech Stack 表 |
| `DESIGN.md` | Node Status Contract 中「身份存放位置」的文案判据 |

**安全影响（需在文档中如实陈述）**

私钥从「系统安全存储」降级为「用户目录下 0600 的明文文件」，形态等同于无 passphrase 的
`~/.ssh/id_ed25519`。防护边界从「同用户下的其他进程读不到」收窄为「其他用户读不到」。
泄露后果：可冒充本机设备身份，已配对设备会信任它。
