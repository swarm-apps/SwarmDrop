## Why

邀请的生命周期是按「当面扫码」设计的，三条参数叠在一起：

- TTL 300 秒（`INVITE_TTL_SECS`）
- 一次性消费（CAS `Pending → Consumed`）
- **纯内存态**（`InviteRegistry` 是 `Mutex<HashMap>`，注释里写明「重启丢邀请是可接受语义」）

对当面扫码这是恰当的。对「发个链接给同事」是灾难：

| 场景 | 现在会发生什么 |
|---|---|
| 微信发链接，对方 10 分钟后点开 | 邀请已过期 |
| 发出去之后自己重启了 App | 邀请已过期（内存态） |
| 对方点了链接，先去装桌面版再打开 | 装完早就过期了 |
| 群里发，另一个人先点了 | 一次性已被消费 |

`invite-url-canonical` 把邀请从「近场即时」推到「异步远程」——链接可点、相机可扫、深链可跳。
生命周期不跟着改的话，**新流程的主要产出会是「邀请已过期」这个页面**，入口做得再顺也救不回来。

这是「配对不好用」的另一半根因，与载体形态正交，可并行推进。

## What Changes

- **TTL 300s → 24h，单一 profile**。不做「近场 300s / 远程 24h」双 profile —— 见 design D1。
- **`InviteRegistry` 落盘**：从纯内存改为经 `InviteStore` 端口持久化。
  - 端口 trait 定义在 `crates/invite`（仍 wasm-clean，trait 不带实现）
  - native 实现在 `crates/storage-sql` + 一张新表 + migration
  - wasm 实现在 `crates/web`，走既有的「内存读缓存 + IndexedDB 写穿」路子
    （`crates/web/src/store.rs` 是现成模板）
  - 重启后未过期、未消费的邀请仍然有效
- **「已发出的邀请」列表 + 主动撤销**（三端）。24h 窗口把 capability 的暴露时间拉长了 48 倍，
  可见性与撤销能力从「可选」变成**必需配套** —— 见 design D3。`revoke_invite` 命令已存在
  （`src-tauri/src/commands/pairing.rs:95`、`crates/web/src/node.rs:332`），本期是补 UI 与列表查询。
- **过期清理**：`prune_expired` 从 lazy 调用改为启动时清一次 + 落盘侧按需清，避免表无限增长。

**非目标**：载体形态与 URL（→ `invite-url-canonical`）；深链与剪贴板（→ `pair-deep-link`）；
把 capability 明文落盘（**永不** —— 见 design D4）。

## Capabilities

### New Capabilities

- `invite-lifecycle`: 配对邀请的有效期与持久化 —— 24 小时 TTL、跨重启存活、一次性消费语义
  不变、发起方可见已发出邀请并主动撤销；capability 明文永不落盘或进日志。

## Impact

- **`crates/invite`**：`INVITE_TTL_SECS` 改 24h；新增 `InviteStore` trait；`InviteRegistry`
  改为持有 store 并在 `register` / `try_consume` / `revoke` / `prune_expired` 时写穿；
  新增「列出未过期未消费邀请」的查询（供 UI）。
- **`crates/storage-sql`**：`InviteStore` 的 SeaORM 实现；`crates/entity` 加 entity；
  `crates/migration` 加一张表（capability 哈希、inviter、expires_at、状态、创建时间）。
- **`crates/web`**：`InviteStore` 的 IndexedDB 实现（新 object store），沿用 `SendWrapper`
  裹 `JsFuture` 满足 `#[async_trait]` 的 Send 约束（`storage-abstraction.md` 有记录）。
- **`crates/core`**：`PairingManager` 组装时注入 store。
- **`src-tauri`**：新增「列出已发出邀请」命令；`setup.rs` 的 `collect_commands!` 登记。
- **桌面 / 移动 / Web 前端**：邀请列表 + 撤销按钮；倒计时文案从「5 分钟」改为相对时间
  （「23 小时后失效」）。
- **回归**：`cargo test --workspace`、`./scripts/check-wasm.sh`、重启后邀请仍可用的冒烟、
  撤销后立即失效的冒烟、并发双花仍恰好一方成功（一次性语义不能因落盘而破）。

**风险**：

1. **一次性语义与落盘的原子性**。现在的 CAS 在单个 `Mutex` 内完成，落盘后要保证
   「检查-置换-写库」不出现窗口 —— 两台设备同时消费同一邀请必须恰好一方成功。
   设计上仍以内存表为权威 CAS 点，落盘是其后置写穿（见 design D2）。
2. **24h 窗口内 capability 的暴露面变大**，靠一次性 + 撤销 + 验签 + 只存哈希共同兜底。
