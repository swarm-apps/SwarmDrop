# 命令行宿主（`crates/cli`）

> 第四个宿主，与桌面 / 移动 / Web 并列。bin 名 `swarmdrop`，package 名 `swarmdrop-cli`。
> **碰 `crates/cli`、`crates/host-fs`、`dist-workspace.toml` 时必读。**

## 它复用了什么、自己做了什么

复用面比预想的大：`swarmdrop_core::runtime::start_node` 这个组合根本来就是平台中立的，
`HostPorts` 只有 5 个字段，`notifier` 还是 `Option`（注释明写「`None` = 该端没有这个概念」）。

| 端口 | CLI 怎么给 |
|---|---|
| `device_config` / `paired_device_store` / `KeychainProvider` / `FileAccess` | **全部复用 `crates/host-fs`**，只提供路径 |
| `event_bus` | 自己实现（`adapter/events.rs`），带订阅能力 |
| `notifier` | `None` |
| `invite_store` / `TransferStore` | `storage-sql` 现成 |

**没有 DTO 层**：CLI 不跨 FFI，直接用 core 的类型。桌面 1613 行 + 移动 2581 行的那层
「命令」代码是 specta / uniffi 强加的类型翻译，**不是可复用的业务逻辑**——CLI 对应的量接近 0。

## 端口的 native 实现住在 `crates/host-fs`

`crates/host` 是**纯端口**（trait + DTO + error + device 类型，零文件 IO）；
本地文件系统的实现全在 `crates/host-fs`：

- `JsonFileIdentityStore` —— 身份 + 已配对设备（原子写、unix 0600、**读取失败不降级**）
- `JsonFileDeviceConfig` —— 设备名
- `LocalFileAccess` —— 读源、暂存、发布（含符号链接越界防护）

**依赖方向**：core → 端口；**宿主 → 端口 + 实现**。core 刻意不依赖 `host-fs`——它要过 wasm
双 target 门禁，而实现是 native-only。谁用实现谁声明它。

⚠️ 拆分时发现的事实：桌面的 `FileAccess` 实现**早就不用 Tauri 了**，7 处 `tauri` 全是未使用的
`_app` 参数。三层结构（trait 实现 → 单变体 enum 分派 → 路径操作）中间那层是历史遗留，
迁移时跨过了它。目录扫描（`enumerate_dir`）留在桌面——它不属于 `FileAccess` 契约，
返回形状是宿主界面的事。

## 单实例：通道发现 + 文件锁仲裁，**不要 pidfile**

同一份数据目录任一时刻最多一个进程持有节点。这是**正确性要求**：同一身份两个进程上线会让
DHT 可达性记录互相覆盖、relay 预留互相顶替、已配对设备表并发写。

```
通道能连上 ──────────────────────▶ 有活节点，走 IPC
     │ 连不上
     ▼
判为陈旧残留 ──▶ 抢文件锁 ──┬─ 拿到 ──▶ 清理残留，我持有节点
                           └─ 没拿到 ──▶ 有并发者正在启动，回到第一步重连
```

**两段缺一不可**：只有通道判定时，两个进程可能同时判出「陈旧」然后一起启动；只有文件锁时，
无法区分「锁没人拿」与「持锁进程已崩溃」。

**pidfile 是错的**：PID 会被复用，陈旧 pidfile 会把「没有节点」误判成「有节点」，
用户陷入「怎么都起不来、也没有进程可杀」的死局。

**文件锁用标准库**（`std::fs::File::try_lock`，1.89 起稳定），不需要 `fs4` 之类的 crate ——
一度加了 fs4，结果发现 `file.try_lock()` 被解析到了标准库那个方法上。

## 节点生命周期与三端同语义

`start` / `stop` / `status` 直接对应核心的 `NodeStatus::{Running, Stopped}`，是三端 UI
（`NodeStatusSheet` / `NodeControlSheet` / Web 弹窗）之外的第四份实现。

**没有 `recv` 命令**：接收是节点在线时的被动后台行为，与「配对 + 被动接收」模型一致。
常驻节点靠 `runtime/receive.rs` 自动接受入站传输——没有界面可以弹确认框，等一个不会到来的
人工确认只会让对端一直卡着。

**不得有隐式常驻启动**。这条与前端的 `pnpm check:node-lifecycle` 是同一条原则
（「那会长成收敛环，用户点了停止立刻被拉回」）。因此：

| | 常驻节点 | 临时节点 |
|---|---|---|
| 谁起的 | `start` | 无节点时的一次性命令 |
| 何时止 | `stop` 或信号 | 命令结束 |
| 都持锁、都开通道 | 是 | 是 |

`status` 不区分两者（设备此刻确实在线）；`stop` 对两者都生效（显式意图优先）。

## 本地通道的两个坑

1. **必须并发处理连接**。`send` 会阻塞到传输终态（可能几分钟），串行处理时那期间连 `stop`
   都递不进来，用户只能杀进程。
2. **`stop` 要留排水窗口**。应答在独立任务里写，而主循环一跳出就关停节点、进程随即退出，
   未写完的应答会随运行时一起消失。现在关停前等 200ms；**不能改成「等那个任务结束」**——
   同一批在途请求里可能有个正在传几分钟的 `send`。

## 邀请的有效期 = 签发者的在线时长

`pair` 生成的邀请里带的是**签发者当时的可拨地址**。所以：

- 常驻节点在跑 ⇒ **必须由它签发**（经通道），否则另起的临时节点一退出，那张码就指向一个
  不存在的节点
- 没有常驻节点 ⇒ 临时节点签发后**必须保持在线**直到配对完成或用户中断

⚠️ 最初的实现是「签完就关节点」，产出的是一张**扫了也拨不通的码，且没有任何报错**——
用户只会看到对方说「连不上」。这类缺陷编译器和单测都发现不了，只有把「邀请里到底带了
什么」想清楚才会意识到。

## `inbox` 不能无条件直连数据库

`migration` 的连接**不设 `journal_mode`**（sqlx 默认，走 SQLite 的 `delete` 模式），
那模式下**写事务会阻塞所有读**。常驻节点接收文件时一直在写，此时直连库的 `inbox list`
会以 `database is locked` 失败。

判据因此是「有没有常驻节点」而不是「要不要节点」：

- 有常驻节点 → 走通道（它自己读，没有并发问题）
- 没有 → 直连库（此时没有并发写者，且不必为看一眼收件箱去起一个 P2P 节点、连引导节点）

通道刚断开的竞态也要兜住：回落直连而不是报错——那一刻恰恰是直连最安全的时候。

## 分发（`dist`）的三个坑

1. **tag 必须用斜杠**：`cli/v0.1.0`。`tag-namespace = "cli"` ≠ 包名 `swarmdrop-cli` 时，
   连字符形式 `cli-v0.1.0` 会被 dist 整串当版本号解析并报
   `Couldn't parse the version ... unexpected character 'c'`。
2. **workspace 里其余 bin crate 必须显式排除**。`dist-workspace.toml` 的 `members` 只列 CLI，
   但 dist 仍会扫描每个有 bin 的包；给它们补了 `repository` 之后就会被纳入发布计划
   （`dist plan` 多 announce 一个 v0.23.0，那是 Tauri 桌面端）。
   `src-tauri` 与 `crates/bootstrap` 因此各带一段 `[package.metadata.dist] dist = false`。
3. **`tag-namespace` 顺带解决了 workflow 命名冲突**：产出的是 `cli-release.yml`，
   不会覆盖既有的 `release.yml`（桌面 Tauri 发版）。改配置后重跑 `dist generate` 前，
   建议先记一份 `release.yml` 的 sha256 用于核对。

参照实现：`../SwarmHive/dist-workspace.toml`（同家族项目，配置与踩坑注释可直接对照）。

### 本地能验证到哪一步

装上 zig + cargo-zigbuild + cargo-xwin 之后，`dist build --artifacts=all` 能在 macOS 上
产出**全部六个平台**的归档、installer、npm 包与 homebrew formula——不需要发布。

⚠️ 过程中会打印 `× unable to run linkage report for <linux-target> on macos`，
**那是非致命警告**：退出码仍是 0、产物齐全。CI 用 `matrix.runner` 按 target 分配原生
runner，不会走到那条路径。第一次见到它很容易误判成「本地验证不了」而直接去发版。

带 tag 构建才能验证下载地址：`dist build --tag cli/v0.1.0`。不带 tag 时 npm 包里的
`artifactDownloadUrls` 是 `releases/download/v0.1.0`，**少了 namespace 段**，
带上才是正确的 `releases/download/cli/v0.1.0`。

## 接收落点

默认 `<下载目录>/SwarmDrop`，`SWARMDROP_RECEIVE_DIR` 覆盖。
**不落进数据目录**——那是应用私有区，用户在文件管理器里翻不到，收到的文件等于丢了。
用环境变量而非配置文件做覆盖：命令行宿主常跑在脚本与服务单元里。
