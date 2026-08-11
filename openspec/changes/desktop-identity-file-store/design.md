## Context

桌面身份存储当前是编译期二选一（`src-tauri/src/host.rs:48`）：

```rust
#[cfg(debug_assertions)]      type DesktopSecretStore = file_keychain::FileKeychainProvider;
#[cfg(not(debug_assertions))] type DesktopSecretStore = keychain::DesktopKeychainProvider;
```

debug 走明文文件，正是因为 dev 二进制的 ad-hoc 签名访问不了 login keychain
（`errSecInteractionNotAllowed`）。而 release 产物的签名标识是 `"-"` —— 同样是 ad-hoc，
只是失败形态从「拒读」变成了「每次弹框」。也就是说，**两个构建面对的是同一个签名问题，
只有 debug 那侧承认了它**。本变更把 debug 那侧的结论推广到 release。

端口层不动：`KeychainProvider` 与 `PairedDeviceStore` 两个 trait 保持原样，换的只是桌面
实现。将来若拿到 Developer ID，切回 keychain 是替换一个实现，`keychain.rs` 那份代码在
git 历史里完整可取。

## Goals / Non-Goals

**Goals:**
- 桌面启动全程零用户交互，且该性质不依赖代码签名状态。
- 私钥文件在任何时刻断电都不会损坏或丢失（原子写）。
- 密钥材料与业务数据分文件，高频写不擦写承载私钥的文件。
- 对外表述（文档站 / README / 应用内诊断 / CLAUDE.md / DESIGN.md）与实现一致。
- 端口边界不变，切回 keychain 的成本仍是「换一个实现」。

**Non-Goals:**
- 不做任何形式的应用层加密。用什么密钥加密私钥、那个密钥又存哪里——这是个循环，
  密码解锁流程已于早前移除，不重新引入。
- 不改移动端与 Web 端的存储形态。
- 不写从 keychain 的迁移（见 D4）。
- 不解决签名问题本身。若将来买了 Developer ID，那是另一个变更，且到时可以整体切回 keychain。

## Decisions

### D1：三个桌面平台统一走文件，不按平台分叉

**选择**：macOS / Windows / Linux 全部使用文件后端。

**备选**：只有 macOS 换文件，Windows / Linux 保留 `keyring`。

**理由**：
- **per-app ACL 只有 macOS 有。** Windows 凭据管理器按用户账户隔离，同一用户下的任何进程
  都能读取——它提供的保护本来就接近于「0600 文件」。Linux Secret Service 在 keyring 锁定
  时确实加密，是三者中唯一有实质损失的，但桌面会话通常在登录时自动解锁。
- 按平台分叉意味着**永久携带**一个 `cfg(target_os)` 的存储后端分叉，而 `host.rs:48` 那段
  注释记着上一次同类分叉的教训：两个分支各写一份逐字相同的代码，漏改一处就是
  「密钥走文件、设备列表走 keychain」这类只在单一构建下出现的错配。
- 保留 `keyring` 依赖意味着 Linux 侧继续携带 secret-service / D-Bus 那条运行时链路。

**注**：这条是本 design 中唯一存在合理反对意见的决策（Linux 侧有实质安全损失）。
若倾向保留 Linux 的 keyring，需在 apply 前推翻本条，其余决策不受影响。

### D2：拆成 `identity.json` + `paired-devices.json` 两个文件

dev 后端把四项状态放在一个 `dev-identity.json` 里。生产不沿用，因为两类数据的**写入频率
相差一个数量级**：

| 文件 | 内容 | 写入时机 |
|---|---|---|
| `identity.json` | Ed25519 keypair、WebRTC Direct 证书 PEM | 首次生成后几乎不再写 |
| `paired-devices.json` | 已配对设备列表 | 配对、解除配对、identify 观察到对端改名 |

合在一起意味着每次对端改名都要重写承载私钥的那个文件。**私钥文件应当近乎只读**——
它每被写一次，就多一个损坏窗口，而它的损坏是不可恢复的（身份丢失 = 所有配对失效）。

端口早已是分开的两个 trait，两个文件恰好让实现与端口同构。

### D3：原子写（临时文件 + rename），仅对 `identity.json` 强制

dev 后端用 `fs::write` 直接覆盖目标文件——写到一半断电就是一个截断的 JSON。
生产实现改为：写入同目录临时文件 → `sync_all()` → `rename` 覆盖目标。同一文件系统内的
`rename` 在 POSIX 与 Windows 上都是原子替换。

`paired-devices.json` 同样采用（成本相同），但它的损坏是可恢复的（大不了重新配对），
所以判据只对 `identity.json` 强制：**该文件必须要么是上一个完整版本，要么是新的完整版本，
不存在第三种状态。**

### D3b：读取失败 MUST NOT 降级为「无身份」

dev 后端的 `read()` 是刻意容错的——解析失败 `warn!` 一句然后返回 `DevIdentityFile::default()`，
于是 `load_identity()` 返回 `Ok(None)`，core 走「生成新身份」的路径。注释写得很清楚，
那是为了让 dev 环境永远能起来。

**这在生产是灾难**：一次 JSON 解析故障（磁盘坏块、被外部工具改坏、格式不兼容的降级安装）
会让应用生成新身份**并覆盖掉原文件** —— 原身份不可恢复，所有配对失效，而用户看到的只是
「设备列表空了」，没有任何错误提示。

生产实现必须区分三种情况：

| 情况 | 行为 |
|---|---|
| 文件不存在 | 生成新身份（首次启动的正常路径） |
| 文件存在且可解析 | 使用它 |
| **文件存在但不可解析** | **报错，不生成、不覆盖** |

这是与原子写并列的第二个「dev 代码提为生产必须补上」的差异：原子写防的是自己写坏，
这条防的是把别的原因造成的坏当成「没有」。
`initialize_identity` 的失败路径已经存在（`secret-store.ts:57` 会把 `initError` 落进
状态并阻止后续启动），接上即可。

### D4：不写迁移，存量 keychain 身份直接作废

已确认 v0.16.x 除作者本人外无真实用户。一次性迁移代码的代价不是写它，而是**长期携带它**：
它只在一台机器上跑过一次、之后永远是死代码，且删除时机没有判据（无从知道存量是否清零）。

与 Web 端「schema 变更直接换，不写迁移 / 回填 / 双写」是同一个口径。

**代价明示**：升级后 `device_id` 变化，所有已配对设备失效，需重新配对。这一条必须写进
发版说明——用户看到的现象是「设备列表空了」，没有说明的话会被当成 bug。

### D5：`IdentityMigrationState` 端口整体删除

`load_or_create_identity` 只在首次生成身份时 `save_migration_state(Completed)`，
**生产代码零读者**（`crates/core/src/identity.rs` 只写，读只出现在测试里）。
它是 Stronghold → keychain 那次迁移留下的墓碑。

删除范围：`crates/host` 的端口方法与 `IdentityMigrationState` 类型、`identity.rs` 的写入调用、
三端实现里对应的方法体（桌面文件后端、移动 uniffi 桥、Web），以及 keychain 里那条存储位。

### D6：身份文件落 `app_local_data_dir`，不落 `app_data_dir`

`paths.rs` 现有两个入口：`app_data_dir`（identity / device_config）与
`app_local_data_dir`（SQLite）。在 macOS 与 Linux 上两者解析到同一个目录，**只有 Windows
不同**：前者是 `%APPDATA%`（Roaming），后者是 `%LOCALAPPDATA%`。

漫游配置文件会把 `%APPDATA%` 同步到域控服务器——**私钥不该跟着漫游**。因此身份文件改用
`app_local_data_dir`。这不是平台分叉：三平台都写同一行代码，只是在 Windows 上恰好落到了
正确的那个目录。

`device_config.json`（设备名）不动，它是可漫游的偏好。

### D7：Windows / Linux 的文件权限

`0600` 只在 unix 上可设（dev 后端已有 `#[cfg(unix)]` 的 `set_permissions`）。
Windows 上不设显式 ACL，依赖 `%LOCALAPPDATA%\{identifier}` 的继承权限——该目录默认只有
当前用户与 SYSTEM 可访问。**不引入 Windows ACL 操作**：正确设置 DACL 需要 winapi 层的
代码，出错的后果（把文件设成任何人可读，或设成自己都读不了）比它防住的风险更大。

### D8：命名同步

模块名 `file_keychain` 与文件名 `dev-identity.json` 都带着「这是权宜之计」的痕迹，
提为生产后必须改名，否则下一个读代码的人会以为生产走的是 keychain。
建议 `host/identity_store.rs`，文件 `identity.json` / `paired-devices.json`。
`e2e` 与 `seed_demo_profile` 里硬编码的旧文件名同步更新。

## Risks / Trade-offs

- **[私钥以明文落盘，同用户下的其他进程可读]** → 如实陈述，不粉饰。形态等同无 passphrase
  的 `~/.ssh/id_ed25519`，是业界普遍接受的形态。文档站与 README 的表述改为准确描述
  （文件位置 + 权限 + 它保护什么、不保护什么），而不是换一个同样模糊的说法。

- **[Linux 用户失去 Secret Service 在 keyring 锁定时的加密]** → D1 已记录这是本变更中唯一
  有实质安全损失的平台。若判断该损失不可接受，推翻 D1 即可，其余决策独立成立。

- **[备份 / 云同步带走私钥文件]** → D6 的 `app_local_data_dir` 挡住了 Windows 漫游；
  macOS 的 `~/Library/Application Support/` 不进 iCloud Drive（除非应用显式声明）；
  Linux 的 `~/.local/share/` 取决于用户自己的备份策略。剩余风险接受并记录。

- **[表述漂移：改了实现没改文档]** → 生物识别那次的原样重演风险。缓解是把 6 处表述
  列进 `tasks.md` 的独立任务组，与实现同 PR 合入，不留「后续再改」。

- **[`SWARMDROP_DATA_DIR` fixture 覆盖的门控]** → 它当前是 `#[cfg(debug_assertions)]`，
  身份文件成为生产存储后该门控**更应该保留**（release 下不允许环境变量重定向身份位置，
  否则是一个可被滥用的身份注入面）。实现时确认这一点，不要顺手放开。

## Open Questions

- D1 的 Linux 侧取舍：三平台统一（当前取定）还是保留 Linux 的 keyring。默认按统一实施，
  apply 前可推翻。
